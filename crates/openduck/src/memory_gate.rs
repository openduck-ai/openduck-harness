use std::sync::atomic::{AtomicBool, Ordering};

static BLOCKED: AtomicBool = AtomicBool::new(false);

pub(crate) fn is_blocked() -> bool {
    BLOCKED.load(Ordering::SeqCst)
}

pub(crate) fn set_blocked(blocked: bool) {
    BLOCKED.store(blocked, Ordering::SeqCst);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryPressureError;

impl std::fmt::Display for MemoryPressureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "Harness paused: memory is about to run out. Running tasks were stopped and new tasks are refused until memory recovers.",
        )
    }
}

impl std::error::Error for MemoryPressureError {}

pub(crate) fn ensure_open() -> Result<(), MemoryPressureError> {
    if is_blocked() {
        Err(MemoryPressureError)
    } else {
        Ok(())
    }
}
