//! Remembering and restoring the frontmost application/window.
//!
//! Pasting sends ⌘V/^V to whatever is frontmost. Anything that gives our own windows
//! focus mid-dictation — clicking Stop on the overlay, opening Settings — would send
//! the text to us instead. So the target is captured when recording starts and
//! reactivated just before the keystroke.

/// Where the pasted text should go.
///
/// `App` stores the platform's native target identifier in a wide integer: a process id
/// on macOS and a window handle on Windows. Deliberately not an `Option`: "we know
/// there is nowhere to paste" and "this platform does not track focus" need opposite
/// handling, and collapsing them into `None` silently disables pasting everywhere focus
/// tracking is not implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetApp {
    /// Another app/window was frontmost; bring it back before the keystroke.
    App(i64),
    /// One of our own windows was frontmost, so the text has nowhere to land.
    OurOwnWindow,
    /// Focus is not tracked here; paste into whatever happens to be frontmost.
    Untracked,
}

impl TargetApp {
    /// Whether pasting can deliver the text somewhere the user will see it.
    pub fn can_receive_paste(self) -> bool {
        !matches!(self, TargetApp::OurOwnWindow)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::TargetApp;
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

    /// Frontmost app at the moment of the call.
    pub fn capture() -> TargetApp {
        let ours = std::process::id() as i32;
        let Some(frontmost) = (unsafe { NSWorkspace::sharedWorkspace().frontmostApplication() })
        else {
            return TargetApp::Untracked;
        };
        let pid = unsafe { frontmost.processIdentifier() };
        let name = unsafe { frontmost.localizedName() }
            .map(|n| n.to_string())
            .unwrap_or_else(|| "unknown".into());

        if pid == ours {
            // Starting a take from our own window means there is nowhere to paste into.
            // Naming it in the log turns "the paste silently did nothing" into a
            // diagnosable event.
            log::info!("take started from our own window; no paste target");
            return TargetApp::OurOwnWindow;
        }
        log::info!("paste target: {name} (pid {pid})");
        TargetApp::App(pid as i64)
    }

    /// Brings the captured app back to the front. Returns false when it has since quit.
    pub fn restore(target: i64) -> bool {
        let pid = target as i32;
        let Some(app) =
            (unsafe { NSRunningApplication::runningApplicationWithProcessIdentifier(pid) })
        else {
            return false;
        };
        // `ignoringOtherApps` is deprecated and inert from macOS 14 on; `allWindows`
        // is the supported way to bring the target app forward.
        unsafe {
            app.activateWithOptions(NSApplicationActivationOptions::NSApplicationActivateAllWindows)
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::TargetApp;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
    };

    /// Capture the exact foreground HWND before our overlay has any chance to receive
    /// focus. A PID is not precise enough on Windows because one process can own several
    /// top-level windows and Ctrl+V must return to the exact editor that was active.
    pub fn capture() -> TargetApp {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd as isize == 0 {
            log::warn!("could not determine the foreground window; paste target untracked");
            return TargetApp::Untracked;
        }

        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid == std::process::id() {
            log::info!("take started from our own Windows window; no paste target");
            return TargetApp::OurOwnWindow;
        }

        log::info!("Windows paste target: hwnd={} pid={pid}", hwnd as isize);
        TargetApp::App(hwnd as isize as i64)
    }

    /// Reactivate the exact window captured at take start. `SetForegroundWindow` may
    /// legally fail when Windows' foreground-lock rules refuse a process that did not
    /// receive recent user input; in that case the caller falls back to the current
    /// foreground window instead of treating delivery as a hard failure.
    pub fn restore(target: i64) -> bool {
        let hwnd = target as isize as HWND;
        if unsafe { IsWindow(hwnd) } == 0 {
            return false;
        }
        unsafe { SetForegroundWindow(hwnd) != 0 }
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
mod platform {
    use super::TargetApp;

    /// Linux focus tracking is not implemented yet. Paste into whichever application is
    /// frontmost when dictation finishes.
    pub fn capture() -> TargetApp {
        TargetApp::Untracked
    }

    pub fn restore(_target: i64) -> bool {
        false
    }
}

pub use platform::capture;

/// Reactivates the target and gives the window server a moment to finish the switch —
/// a keystroke sent immediately after activation can still land on the old app.
pub fn restore_and_settle(target: TargetApp) {
    match target {
        TargetApp::App(native_target) => {
            if platform::restore(native_target) {
                std::thread::sleep(std::time::Duration::from_millis(120));
                log::info!("reactivated paste target {native_target}");
            } else {
                log::warn!(
                    "paste target {native_target} is gone or could not be activated; \
                     pasting into whatever is frontmost"
                );
            }
        }
        TargetApp::Untracked => {}
        TargetApp::OurOwnWindow => {
            log::warn!("restore called with no paste target; this should not happen")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TargetApp;

    /// Regression: `Untracked` once shared a representation with "nowhere to paste",
    /// which disabled pasting entirely on every platform without focus tracking.
    #[test]
    fn only_our_own_window_blocks_pasting() {
        assert!(TargetApp::App(42).can_receive_paste());
        assert!(TargetApp::Untracked.can_receive_paste());
        assert!(!TargetApp::OurOwnWindow.can_receive_paste());
    }
}
