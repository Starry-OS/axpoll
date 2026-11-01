//! Reproduces the "wake-while-locked" hazard in `PollSet`.
//!
//! `PollSet::wake` carefully drops the internal spin lock before waking the
//! registered wakers, but `PollSet::register` does not: when the ring buffer
//! is full, `Inner::register` evicts an old waker and calls `old.wake()` while
//! the lock is still held. If the woken task re-enters the same `PollSet` (for
//! example by re-registering or polling again), it deadlocks on the
//! non-reentrant spin lock.

use std::{
    sync::Arc,
    sync::mpsc,
    task::{Wake, Waker},
    thread,
    time::Duration,
};

use axpoll::PollSet;

/// A waker that re-enters the `PollSet` whenever it is woken.
struct ReentrantWaker {
    ps: Arc<PollSet>,
}

impl Wake for ReentrantWaker {
    fn wake(self: Arc<Self>) {
        let waker = Waker::from(self.clone());
        self.ps.register(&waker);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let waker = Waker::from(self.clone());
        self.ps.register(&waker);
    }
}

/// A distinct no-op waker.
struct Dummy;

// We need *distinct* wakers (not `Waker::noop()`, which is a singleton and
// would be deduplicated), so a manual no-op impl is intentional here.
#[allow(clippy::manual_noop_waker)]
impl Wake for Dummy {
    fn wake(self: Arc<Self>) {}
    fn wake_by_ref(self: &Arc<Self>) {}
}

#[test]
fn eviction_must_not_wake_under_lock() {
    let ps = Arc::new(PollSet::new());

    // Fill 64 slots: slot 0 is the reentrant waker, the rest are distinct dummies.
    let reentrant = Arc::new(ReentrantWaker { ps: ps.clone() });
    let reentrant_waker = Waker::from(reentrant);
    ps.register(&reentrant_waker);
    for _ in 0..63 {
        ps.register(&Waker::from(Arc::new(Dummy)));
    }

    // The 65th (distinct) registration evicts slot 0 and wakes it. If
    // `register` wakes the evicted waker while holding the lock, the reentrant
    // waker deadlocks and this thread never sends on the channel.
    let (tx, rx) = mpsc::channel();
    let ps = ps.clone();
    let handle = thread::spawn(move || {
        ps.register(&Waker::from(Arc::new(Dummy)));
        let _ = tx.send(());
    });

    let completed = rx.recv_timeout(Duration::from_secs(2)).is_ok();
    if completed {
        let _ = handle.join();
    }
    assert!(
        completed,
        "register() woke the evicted waker while still holding the lock (deadlock)"
    );
}
