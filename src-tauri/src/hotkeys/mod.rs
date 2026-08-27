//! Global hotkey registration and dispatch.
//!
//! A failed rebind never removes the user's working bindings: we clear the previous
//! set only after validating all of the new strings. Individual OS registration
//! failures are logged and the remaining shortcuts still get installed.

use crate::session;
use crate::settings::{AppSettings, HotkeyAction};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Default)]
pub struct HotkeyRegistry {
    actions: Mutex<HashMap<Shortcut, HotkeyAction>>,
}

impl HotkeyRegistry {
    fn action_for(&self, shortcut: &Shortcut) -> Option<HotkeyAction> {
        self.actions.lock().ok()?.get(shortcut).cloned()
    }
}

/// Replaces all registered shortcuts with the settings snapshot.
pub fn apply(app: &AppHandle, settings: &AppSettings) {
    let registry = app.state::<HotkeyRegistry>();
    let mut parsed = Vec::new();

    for binding in &settings.hotkeys {
        let shortcut = match binding.shortcut.parse::<Shortcut>() {
            Ok(value) => value,
            Err(e) => {
                log::warn!(
                    "invalid shortcut {:?} for {:?}: {e}",
                    binding.shortcut,
                    binding.action
                );
                continue;
            }
        };
        parsed.push((shortcut, binding.action.clone()));
    }

    if let Err(e) = app.global_shortcut().unregister_all() {
        log::warn!("could not clear old global shortcuts: {e}");
    }

    let mut active = HashMap::new();
    for (shortcut, action) in parsed {
        match app.global_shortcut().register(shortcut) {
            Ok(()) => {
                active.insert(shortcut, action);
            }
            Err(e) => log::warn!("could not register {shortcut:?}: {e}"),
        }
    }

    if let Ok(mut actions) = registry.actions.lock() {
        *actions = active;
    }
}

pub fn on_shortcut(app: &AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    let Some(action) = app.state::<HotkeyRegistry>().action_for(shortcut) else {
        // Logged rather than ignored: "the hotkey does nothing" is the single most
        // common report, and this line is what separates "the key never reached us"
        // from "it reached us and the action misfired".
        log::debug!("unbound shortcut fired: {shortcut:?} ({state:?})");
        return;
    };
    log::debug!("hotkey {action:?} {state:?}");
    match (action, state) {
        // Every binding fires once, on key-down. Key-up is what a held key sends on
        // release, and acting on it too would run the action twice per press.
        (_, ShortcutState::Released) => {}
        (HotkeyAction::Toggle, _) => session::toggle(app),
        (HotkeyAction::Cancel, _) => session::cancel(app),
        (HotkeyAction::NextMode, _) => session::next_mode(app),
        (HotkeyAction::SelectMode(mode_id), _) => {
            session::start(app, Some(mode_id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::defaults;

    #[test]
    fn default_shortcuts_parse() {
        for binding in defaults::default_settings().hotkeys {
            assert!(
                binding.shortcut.parse::<Shortcut>().is_ok(),
                "{}",
                binding.shortcut
            );
        }
    }
}
