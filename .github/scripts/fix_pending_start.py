from pathlib import Path


def once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected 1 match, got {count}")
    return text.replace(old, new, 1)


p = Path("src-tauri/src/session/mod.rs")
s = p.read_text()
s = once(
    s,
    """    starting: AtomicBool,
    active: Mutex<Option<ActiveTake>>,
""",
    """    starting: AtomicBool,
    start_cancelled: AtomicBool,
    active: Mutex<Option<ActiveTake>>,
""",
    "cancel flag field",
)
s = once(
    s,
    """        self.starting
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(StartReservation {
""",
    """        self.starting
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        self.start_cancelled.store(false, Ordering::Release);
        Some(StartReservation {
""",
    "reservation reset",
)
s = once(
    s,
    """pub fn toggle(app: &AppHandle) {
    if app.state::<Recorder>().is_recording() {
        stop(app);
    } else {
        start(app, None);
    }
}
""",
    """pub fn toggle(app: &AppHandle) {
    if app.state::<Recorder>().is_busy() {
        stop(app);
    } else {
        start(app, None);
    }
}
""",
    "toggle pending start",
)
s = once(
    s,
    """    let Ok(mut slot) = recorder.active.lock() else {
        capture.stop();
        stt_task.abort();
        return false;
    };
    debug_assert!(
""",
    """    let Ok(mut slot) = recorder.active.lock() else {
        capture.stop();
        stt_task.abort();
        return false;
    };
    // A stop/cancel may arrive after the start reservation is taken but before the
    // fully constructed take can be published. Check while holding `active` so there
    // is no gap between honoring that cancellation and publishing the take.
    if recorder.start_cancelled.swap(false, Ordering::AcqRel) {
        capture.stop();
        stt_task.abort();
        return false;
    }
    debug_assert!(
""",
    "pending cancellation check",
)
s = once(
    s,
    """pub fn stop(app: &AppHandle) {
    let Some(mut take) = app
        .state::<Recorder>()
        .active
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
    else {
        return;
    };
""",
    """pub fn stop(app: &AppHandle) {
    let recorder = app.state::<Recorder>();
    let recording = recorder
        .active
        .lock()
        .ok()
        .and_then(|mut slot| slot.take());
    let Some(mut take) = recording else {
        if recorder.starting.load(Ordering::Acquire) {
            recorder.start_cancelled.store(true, Ordering::Release);
        }
        return;
    };
""",
    "stop pending start",
)
s = once(
    s,
    """    if let Some(mut take) = recording {
        take.cancelled.store(true, Ordering::Relaxed);
        take.capture.stop();
        take.audio_tx.take();
        take.stt_task.abort();
        abandon(app, &take.mode_id);
        return;
    }

    // Nothing recording: there may still be a stopped take waiting on the provider.
""",
    """    if let Some(mut take) = recording {
        take.cancelled.store(true, Ordering::Relaxed);
        take.capture.stop();
        take.audio_tx.take();
        take.stt_task.abort();
        abandon(app, &take.mode_id);
        return;
    }

    if recorder.starting.load(Ordering::Acquire) {
        recorder.start_cancelled.store(true, Ordering::Release);
        let _ = web_bridge::deliver_cancelled(app);
        return;
    }

    // Nothing recording: there may still be a stopped take waiting on the provider.
""",
    "cancel pending start",
)
p.write_text(s)

p = Path("src-tauri/src/web_bridge.rs")
s = p.read_text()
s = once(
    s,
    """    } else {
        clear_matching_request(app, &request_id);
        send_protocol_error(&tx, Some(&request_id), "START_FAILED");
        false
    }
""",
    """    } else {
        // If cancel/disconnect already consumed the bridge request while start was
        // pending, it owns the terminal event. Avoid a second START_FAILED message.
        if clear_matching_request(app, &request_id) {
            send_protocol_error(&tx, Some(&request_id), "START_FAILED");
        }
        false
    }
""",
    "deep-link terminal ownership",
)
s = once(
    s,
    """fn clear_matching_request(app: &AppHandle, request_id: &str) {
    let bridge = app.state::<WebBridge>();
    let Ok(mut slot) = bridge.request.lock() else {
        return;
    };
    if slot
        .as_ref()
        .map(|request| request.request_id == request_id)
        .unwrap_or(false)
    {
        slot.take();
    }
}
""",
    """fn clear_matching_request(app: &AppHandle, request_id: &str) -> bool {
    let bridge = app.state::<WebBridge>();
    let Ok(mut slot) = bridge.request.lock() else {
        return false;
    };
    if slot
        .as_ref()
        .map(|request| request.request_id == request_id)
        .unwrap_or(false)
    {
        slot.take();
        true
    } else {
        false
    }
}
""",
    "clear matching result",
)
p.write_text(s)
