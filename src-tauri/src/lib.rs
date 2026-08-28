//! Tocky Voice — realtime speech to text with an optional AI cleanup pass,
//! pasted straight into whatever app has focus.

// `audio`, `settings`, `stt` and `refine` are public so the integration tests in
// `tests/` can drive the provider protocols directly against the live APIs.
pub mod audio;
pub mod refine;
pub mod settings;
pub mod stt;

fn forwarded_tocky_links(argv: &[String]) -> impl Iterator<Item = &str> {
    argv.iter()
        .map(String::as_str)
        .filter(|argument| argument.starts_with("tocky://"))
}

#[cfg(test)]
mod deep_link_arg_tests {
    use super::forwarded_tocky_links;

    #[test]
    fn forwards_only_tocky_protocol_arguments_from_the_secondary_process() {
        let argv = vec![
            "C:\\Program Files\\Tocky\\tockyvoice.exe".to_string(),
            "--flag".to_string(),
            "tocky://listen?request_id=abc&nonce=def".to_string(),
            "https://staging.ptap-next-staging.pages.dev".to_string(),
        ];

        assert_eq!(
            forwarded_tocky_links(&argv).collect::<Vec<_>>(),
            vec!["tocky://listen?request_id=abc&nonce=def"],
        );
    }
}

mod commands;
mod errors;
mod focus;
mod history;
mod hotkeys;
mod inject;
#[cfg(target_os = "macos")]
mod macos_accessibility;
mod overlay;
mod ptap_vocabulary_snapshot;
mod private_file;
mod session;
mod state;
mod terminology;
mod tray;
mod web_bridge;

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut builder = tauri::Builder::default();

    // Deep links on Windows/Linux launch a second process. This plugin must be first so
    // the URL is forwarded to the already-running instance before any other plugin can
    // consume or interfere with startup arguments.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            log::debug!("secondary Tocky instance forwarded arguments: {argv:?}");
            for link in forwarded_tocky_links(&argv) {
                if !web_bridge::handle_deep_link(app, link) {
                    log::warn!("secondary Tocky instance supplied an unhandled deep link");
                }
            }
        }));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        // In-app update check + install. Windows/Linux install and relaunch through this;
        // macOS only ever calls `check()` here — the unsigned bundle can't safely replace
        // itself (see src/lib/update-policy.ts), so `process:allow-restart` backs the
        // Windows/Linux relaunch only.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    hotkeys::on_shortcut(app, shortcut, event.state());
                })
                .build(),
        )
        .manage(session::Recorder::default())
        .manage(web_bridge::WebBridge::default())
        .manage(hotkeys::HotkeyRegistry::default())
        .manage(audio::mic_test::MicTest::default())
        .setup(|app| {
            let handle = app.handle().clone();

            // Menu-bar app: this is what keeps the overlay from stealing focus from the
            // app being dictated into, which would send the paste keystroke to us instead.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings = settings::load(&handle);
            if let Ok(dir) = handle.path().app_data_dir() {
                settings::secrets::configure(dir, settings.use_os_keychain);
            }
            log::info!(
                "starting: stt={:?} llm={}/{} mode={} accessibility={}",
                settings.stt.provider,
                settings.llm.preset,
                settings.llm.model,
                settings.active_mode_id,
                inject::can_synthesize_input(),
            );
            // Written on every launch because "that microphone does not work" cannot be
            // answered without it: the format, rate and channel count all come from the
            // driver and differ per device, and the machine with the problem is rarely
            // the machine with the debugger.
            //
            // Off the setup thread, though. Asking every endpoint for its full config
            // list means one driver query per device, and a machine with several
            // endpoints plus a Bluetooth headset takes seconds — seconds spent before
            // the event loop starts, with no window on screen to explain the wait.
            std::thread::spawn(|| {
                for line in audio::capture::describe_input_devices() {
                    log::info!("input: {line}");
                }
            });
            log::info!(
                "credential store: {}",
                if settings.use_os_keychain {
                    "OS keychain"
                } else {
                    "local vault (0600)"
                }
            );
            app.manage(state::AppState::new(settings.clone()));
            hotkeys::apply(&handle, &settings);
            tray::build(&handle, &settings)?;

            // The localhost bridge is independent of settings/UI startup. Binding errors
            // are logged by the task and never prevent normal hotkey dictation.
            web_bridge::start(&handle);

            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;

                // Installed desktop bundles register the configured scheme. During
                // Windows/Linux development register it for the current executable too.
                #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
                app.deep_link().register_all()?;

                if let Some(urls) = app.deep_link().get_current()? {
                    for url in urls {
                        web_bridge::handle_deep_link(&handle, url.as_str());
                    }
                }

                let deep_link_handle = handle.clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        web_bridge::handle_deep_link(&deep_link_handle, url.as_str());
                    }
                });
            }

            // Under the Accessory activation policy the app never activates on its own,
            // so the settings window would open behind everything else. Ask for focus
            // explicitly at startup — without this the app looks like it launched into
            // nothing but a menu-bar icon.
            show_settings_window(&handle);

            // Ask for Accessibility up front. Without it the paste fails silently, and
            // the system prompt with its "Open System Settings" button is far better
            // than a log line nobody reads.
            #[cfg(target_os = "macos")]
            if !macos_accessibility::prompt_for_accessibility_permission() {
                log::warn!("Accessibility permission is not granted yet");
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the settings window should leave the app running in the menu bar.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::reset_settings,
            commands::list_llm_presets,
            commands::list_input_devices,
            commands::start_mic_test,
            commands::stop_mic_test,
            commands::set_api_key,
            commands::delete_api_key,
            commands::key_status,
            commands::start_recording,
            commands::stop_recording,
            commands::cancel_recording,
            commands::set_overlay_suppressed,
            commands::toggle_recording,
            commands::set_active_mode,
            commands::get_history,
            commands::delete_history_entry,
            commands::clear_history,
            commands::copy_text,
            commands::permission_status,
            commands::open_accessibility_settings,
            commands::open_url,
            commands::test_llm,
            commands::test_stt_key,
            commands::list_models,
            commands::show_main_window,
            commands::suspend_hotkeys,
            commands::resume_hotkeys,
        ])
        .build(tauri::generate_context!())
        .expect("error while starting Tocky Voice")
        .run(|_app, _event| {
            // Clicking the app again in Finder or the Dock sends Reopen rather than
            // launching a second copy. A menu-bar app has to answer it itself, or the
            // relaunch looks like the app silently refused to open.
            //
            // `RunEvent::Reopen` only exists in the macOS build of Tauri — there is no
            // equivalent event on Windows or Linux, so referring to it unconditionally
            // fails to compile there.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                show_settings_window(_app);
            }
        });
}

/// Brings the settings window to the front. The app runs as a menu-bar accessory, so
/// it never activates on its own — without this the window opens behind everything,
/// or a relaunch from Finder appears to do nothing at all.
///
/// Deliberately does *not* switch the activation policy: flipping to `Regular` and
/// back makes the window vanish when the policy reverts.
pub fn show_settings_window(app: &tauri::AppHandle) {
    // Un-hides the whole app on macOS; a no-op elsewhere.
    #[cfg(target_os = "macos")]
    let _ = app.show();

    let Some(window) = app.get_webview_window("main") else {
        log::error!("main window is missing");
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}
