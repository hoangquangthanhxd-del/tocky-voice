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
    "use std::sync::atomic::{AtomicBool, Ordering};",
    "use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};",
    "atomic import",
)
s = once(
    s,
    """const FINALIZE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Default)]
pub struct Recorder {
""",
    """const FINALIZE_TIMEOUT: Duration = Duration::from_secs(20);

const START_IDLE: u8 = 0;
const STARTING: u8 = 1;
const START_CANCELLED: u8 = 2;

#[derive(Default)]
pub struct Recorder {
""",
    "start state constants",
)
s = once(
    s,
    """    starting: AtomicBool,
    start_cancelled: AtomicBool,
    active: Mutex<Option<ActiveTake>>,
""",
    """    start_state: AtomicU8,
    active: Mutex<Option<ActiveTake>>,
""",
    "recorder start fields",
)
s = once(
    s,
    """struct StartReservation<'a> {
    flag: &'a AtomicBool,
}

impl Drop for StartReservation<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}
""",
    """struct StartReservation<'a> {
    state: &'a AtomicU8,
}

impl Drop for StartReservation<'_> {
    fn drop(&mut self) {
        self.state.store(START_IDLE, Ordering::Release);
    }
}
""",
    "reservation type",
)
s = once(
    s,
    """    pub fn is_busy(&self) -> bool {
        self.starting.load(Ordering::Acquire) || self.is_recording()
    }

    fn try_reserve_start(&self) -> Option<StartReservation<'_>> {
        self.starting
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        self.start_cancelled.store(false, Ordering::Release);
        Some(StartReservation {
            flag: &self.starting,
        })
    }
""",
    """    pub fn is_busy(&self) -> bool {
        self.start_state.load(Ordering::Acquire) != START_IDLE || self.is_recording()
    }

    fn try_reserve_start(&self) -> Option<StartReservation<'_>> {
        self.start_state
            .compare_exchange(START_IDLE, STARTING, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(StartReservation {
            state: &self.start_state,
        })
    }

    fn cancel_pending_start(&self) -> bool {
        match self.start_state.compare_exchange(
            STARTING,
            START_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => true,
            Err(START_CANCELLED) => true,
            Err(_) => false,
        }
    }
""",
    "start state helpers",
)
s = once(
    s,
    "if recorder.start_cancelled.swap(false, Ordering::AcqRel) {",
    "if recorder.start_state.load(Ordering::Acquire) == START_CANCELLED {",
    "publish cancellation check",
)
s = once(
    s,
    """        if recorder.starting.load(Ordering::Acquire) {
            recorder.start_cancelled.store(true, Ordering::Release);
        }
        return;
""",
    """        recorder.cancel_pending_start();
        return;
""",
    "stop pending cancellation",
)
s = once(
    s,
    """    if recorder.starting.load(Ordering::Acquire) {
        recorder.start_cancelled.store(true, Ordering::Release);
        let _ = web_bridge::deliver_cancelled(app);
        return;
    }
""",
    """    if recorder.cancel_pending_start() {
        let _ = web_bridge::deliver_cancelled(app);
        return;
    }
""",
    "cancel pending cancellation",
)
s = once(
    s,
    """    fn start_reservation_is_exclusive() {
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
""",
    """    fn start_reservation_is_exclusive() {
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

    #[test]
    fn pending_start_cancellation_cannot_be_reset_by_another_start() {
        let recorder = Recorder::default();
        let first = recorder
            .try_reserve_start()
            .expect("first caller should reserve start");
        assert!(recorder.cancel_pending_start());
        assert_eq!(recorder.start_state.load(Ordering::Acquire), START_CANCELLED);
        assert!(recorder.try_reserve_start().is_none());
        assert_eq!(recorder.start_state.load(Ordering::Acquire), START_CANCELLED);
        drop(first);
        assert_eq!(recorder.start_state.load(Ordering::Acquire), START_IDLE);
        assert!(!recorder.is_busy());
    }
""",
    "race regression test",
)
p.write_text(s)
