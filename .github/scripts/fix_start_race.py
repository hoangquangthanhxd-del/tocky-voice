from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, got {count}")
    return text.replace(old, new, 1)


def replace_first(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise SystemExit(f"{label}: expected at least one match")
    return text.replace(old, new, 1)


session_path = Path("src-tauri/src/session/mod.rs")
text = session_path.read_text()

text = replace_once(
    text,
    """#[derive(Default)]
pub struct Recorder {
    active: Mutex<Option<ActiveTake>>,
    /// Present while a stopped take is still waiting on the provider. Sending on it
    /// abandons that wait — the escape hatch from a socket that never answers.
    finalizing: Mutex<Option<oneshot::Sender<()>>>,
}

impl Recorder {
    pub fn is_recording(&self) -> bool {
        self.active
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }
}
""",
    """#[derive(Default)]
pub struct Recorder {
    /// Atomic reservation for synchronous setup before `active` can hold the fully
    /// constructed take. Two entry points must never open mic/provider concurrently.
    starting: AtomicBool,
    active: Mutex<Option<ActiveTake>>,
    /// Present while a stopped take is still waiting on the provider. Sending on it
    /// abandons that wait — the escape hatch from a socket that never answers.
    finalizing: Mutex<Option<oneshot::Sender<()>>>,
}

struct StartReservation<'a> {
    flag: &'a AtomicBool,
}

impl Drop for StartReservation<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

impl Recorder {
    pub fn is_recording(&self) -> bool {
        self.active
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    pub fn is_busy(&self) -> bool {
        self.starting.load(Ordering::Acquire) || self.is_recording()
    }

    fn try_reserve_start(&self) -> Option<StartReservation<'_>> {
        self.starting
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(StartReservation {
            flag: &self.starting,
        })
    }
}
""",
    "recorder reservation",
)

text = replace_once(
    text,
    """/// Begins recording. `mode_id` switches mode first; `None` keeps the active one.
/// A no-op if a take is already running, so a repeated key press is harmless.
pub fn start(app: &AppHandle, mode_id: Option<String>) {
    let recorder = app.state::<Recorder>();
    if recorder.is_recording() {
        return;
    }
""",
    """/// Begins recording. `mode_id` switches mode first; `None` keeps the active one.
/// Returns true only when this caller established the take. A concurrent start or an
/// already-running take returns false before another microphone/provider is opened.
pub fn start(app: &AppHandle, mode_id: Option<String>) -> bool {
    let recorder = app.state::<Recorder>();
    let Some(_start_reservation) = recorder.try_reserve_start() else {
        return false;
    };
    if recorder
        .active
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(true)
    {
        return false;
    }
""",
    "start reservation",
)

text = replace_once(
    text,
    """        feedback::play(feedback::Cue::Error, settings.audio.feedback_volume);
        return;
    };

    let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<capture::CaptureChunk>();
""",
    """        feedback::play(feedback::Cue::Error, settings.audio.feedback_volume);
        return false;
    };

    let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<capture::CaptureChunk>();
""",
    "missing key return",
)

text = replace_once(
    text,
    """            feedback::play(feedback::Cue::Error, settings.audio.feedback_volume);
            return;
        }
    };

    let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<u8>>();
""",
    """            feedback::play(feedback::Cue::Error, settings.audio.feedback_volume);
            return false;
        }
    };

    let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<u8>>();
""",
    "capture error return",
)

text = replace_once(
    text,
    """    if let Ok(mut slot) = recorder.active.lock() {
        *slot = Some(ActiveTake {
            capture,
            audio_tx: Some(audio_tx),
            cancelled: Arc::new(AtomicBool::new(false)),
            heard_audio,
            pcm,
            mode_id: mode_id.clone(),
            target_app,
            stt_task,
        });
    }
""",
    """    let Ok(mut slot) = recorder.active.lock() else {
        capture.stop();
        stt_task.abort();
        return false;
    };
    debug_assert!(slot.is_none(), "start reservation must prevent active overwrite");
    if slot.is_some() {
        capture.stop();
        stt_task.abort();
        return false;
    }
    *slot = Some(ActiveTake {
        capture,
        audio_tx: Some(audio_tx),
        cancelled: Arc::new(AtomicBool::new(false)),
        heard_audio,
        pcm,
        mode_id: mode_id.clone(),
        target_app,
        stt_task,
    });
    drop(slot);
""",
    "active slot write",
)

text = replace_once(
    text,
    """    state::emit_status(app, Phase::Recording, &mode_id);
}

/// Ends the take and hands the result to the transcript → refine → paste pipeline.
""",
    """    state::emit_status(app, Phase::Recording, &mode_id);
    true
}

/// Ends the take and hands the result to the transcript → refine → paste pipeline.
""",
    "start success return",
)

if "start_reservation_is_exclusive" not in text:
    text += """

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_reservation_is_exclusive() {
        let recorder = Recorder::default();
        let first = recorder
            .try_reserve_start()
            .expect("first caller should reserve start");
        assert!(recorder.is_busy());
        assert!(recorder.try_reserve_start().is_none());
        drop(first);
        assert!(!recorder.is_busy());
        assert!(recorder.try_reserve_start().is_some());
    }
}
"""

session_path.write_text(text)

bridge_path = Path("src-tauri/src/web_bridge.rs")
bridge = bridge_path.read_text()
bridge = replace_first(
    bridge,
    "if app.state::<session::Recorder>().is_recording() {",
    "if app.state::<session::Recorder>().is_busy() {",
    "bridge prepare busy guard",
)
bridge = replace_once(
    bridge,
    """    session::start(app, None);
    if app.state::<session::Recorder>().is_recording() {
        send_json(
""",
    """    if session::start(app, None) {
        send_json(
""",
    "deep-link exact start ownership",
)
bridge_path.write_text(bridge)

commands_path = Path("src-tauri/src/commands.rs")
commands = commands_path.read_text()
commands = replace_once(
    commands,
    """if app.state::<session::Recorder>().is_recording() {
        return Err("a dictation is already running".into());
    }""",
    """if app.state::<session::Recorder>().is_busy() {
        return Err("a dictation is already running".into());
    }""",
    "mic-test busy guard",
)
commands_path.write_text(commands)
