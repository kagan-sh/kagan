use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};

use crate::Grid;
use crate::screen::{Edit, SNAPSHOT_MAX, Screen};

pub(super) struct Frame {
    pub(super) version: u64,
    pub(super) grid: Arc<Grid>,
}

#[cfg(test)]
pub(super) type PollGate = (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>);

// What the reader thread and the terminal share. Versions move and grids are built only with the
// screen locked, so a grid's version is the screen's version when it was built.
#[derive(Clone)]
pub(super) struct Shared {
    pub(super) screen: Arc<Mutex<Screen>>,
    pub(super) frame: Arc<Mutex<Frame>>,
    pub(super) title: Arc<Mutex<Option<String>>>,
    pub(super) versions: Arc<AtomicU64>,
    // A grid was asked for while the screen was busy: the reader builds one after its chunk.
    pub(super) wanted: Arc<AtomicBool>,
    pub(super) edits: Arc<Mutex<Vec<Edit>>>,
    pub(super) changed: Arc<AtomicBool>,
    pub(super) settled: Arc<AtomicBool>,
    pub(super) snapshot_bound: Arc<AtomicUsize>,
    #[cfg(test)]
    pub(super) builds: Arc<AtomicUsize>,
    /// Test only: the reader waits on this before it reports end of file.
    #[cfg(test)]
    pub(super) eof_gate: Arc<Mutex<Option<std::sync::mpsc::Receiver<()>>>>,
    /// Test only: the teardown waits on this before it publishes the final screen.
    #[cfg(test)]
    pub(super) publish_gate: Arc<Mutex<Option<std::sync::mpsc::Receiver<()>>>>,
    // Test only: the reader sends on the first channel when it reaches its poll, then waits on the second.
    #[cfg(test)]
    pub(super) poll_gate: Arc<Mutex<Option<PollGate>>>,
}

impl Shared {
    pub(super) fn settled(mut screen: Screen) -> Self {
        let _ = screen.end_hold();
        let grid = screen.grid();
        let title = screen.title().map(str::to_owned);
        Self {
            screen: Arc::new(Mutex::new(screen)),
            frame: Arc::new(Mutex::new(Frame {
                version: 1,
                grid: Arc::new(grid),
            })),
            title: Arc::new(Mutex::new(title)),
            versions: Arc::new(AtomicU64::new(1)),
            wanted: Arc::new(AtomicBool::new(false)),
            edits: Arc::new(Mutex::new(Vec::new())),
            changed: Arc::new(AtomicBool::new(false)),
            settled: Arc::new(AtomicBool::new(true)),
            snapshot_bound: Arc::new(AtomicUsize::new(SNAPSHOT_MAX)),
            #[cfg(test)]
            builds: Arc::default(),
            #[cfg(test)]
            eof_gate: Arc::default(),
            #[cfg(test)]
            publish_gate: Arc::default(),
            #[cfg(test)]
            poll_gate: Arc::default(),
        }
    }

    pub(super) fn feed(&self, bytes: &[u8]) -> Vec<u8> {
        let mut screen = lock(&self.screen);
        self.drain(&mut screen);
        let reply = screen.feed(bytes);
        self.publish(&screen);
        reply
    }

    // Built only when the screen changed since the last grid. It never waits for the reader: while
    // the reader holds the screen, this is the last grid built.
    pub(super) fn grid(&self) -> Grid {
        if let Some(screen) = try_lock(&self.screen) {
            self.wanted.store(false, Ordering::Relaxed);
            self.build(&screen);
        } else {
            self.wanted.store(true, Ordering::Relaxed);
            self.changed.store(true, Ordering::Release);
        }
        // The frame lock is held only to clone the Arc. The cells are copied without it.
        let grid = Arc::clone(&lock(&self.frame).grid);
        grid.as_ref().clone()
    }

    pub(super) fn drained<T>(&self, read: impl FnOnce(&Screen) -> T) -> T {
        let mut screen = lock(&self.screen);
        self.drain(&mut screen);
        read(&screen)
    }

    /// Applies the queued changes in order and publishes the result. Call with the screen locked.
    pub(super) fn drain(&self, screen: &mut Screen) {
        let edits = std::mem::take(&mut *lock(&self.edits));
        if edits.is_empty() {
            return;
        }
        for edit in edits {
            screen.apply(edit);
        }
        self.publish(screen);
    }

    // Marks the screen changed; the grid waits until it is read. Call with the screen locked.
    pub(super) fn publish(&self, screen: &Screen) {
        self.versions.fetch_add(1, Ordering::Relaxed);
        {
            let mut title = lock(&self.title);
            if title.as_deref() != screen.title() {
                *title = screen.title().map(str::to_owned);
            }
        }
        if self.wanted.swap(false, Ordering::Relaxed) {
            self.build(screen);
        }
        self.changed.store(true, Ordering::Release);
    }

    // Call with the screen locked.
    fn build(&self, screen: &Screen) {
        let version = self.versions.load(Ordering::Relaxed);
        if lock(&self.frame).version >= version {
            return;
        }
        #[cfg(test)]
        self.builds.fetch_add(1, Ordering::Relaxed);
        store_frame(&self.frame, version, screen.grid());
    }
}

/// Replaces the frame unless it is newer. The old grid is dropped after the frame lock is released.
pub(super) fn store_frame(frame: &Mutex<Frame>, version: u64, grid: Grid) {
    let previous = {
        let mut current = lock(frame);
        if version < current.version {
            return;
        }
        std::mem::replace(
            &mut *current,
            Frame {
                version,
                grid: Arc::new(grid),
            },
        )
    };
    drop(previous);
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Takes the lock only if nobody holds it now. A poisoned lock is still taken.
pub(super) fn try_lock<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match mutex.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}
