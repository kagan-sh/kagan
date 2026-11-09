use std::cell::Cell;

thread_local! {
    pub(crate) static DEPTH: Cell<u32> = const { Cell::new(0) };
    pub(crate) static ARMED: Cell<bool> = const { Cell::new(false) };
    pub(crate) static PERSON_BLOCKED: Cell<bool> = const { Cell::new(false) };
}

#[must_use]
pub(crate) struct Armed;

impl Drop for Armed {
    fn drop(&mut self) {
        ARMED.set(false);
    }
}

pub(crate) fn arm() -> Armed {
    ARMED.set(true);
    Armed
}

struct DepthGuard;

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

pub(crate) fn with_active<R>(run: impl FnOnce() -> R) -> R {
    DEPTH.with(|depth| depth.set(depth.get() + 1));
    let _guard = DepthGuard;
    run()
}

pub(crate) fn refuse_for_person() -> bool {
    let synthetic = ARMED.get() || DEPTH.get() > 0;
    if synthetic {
        PERSON_BLOCKED.set(true);
    }
    synthetic
}

pub(crate) fn take_person_blocked() -> bool {
    let blocked = PERSON_BLOCKED.get();
    PERSON_BLOCKED.set(false);
    blocked
}
