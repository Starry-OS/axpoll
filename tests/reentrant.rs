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

#[test]
fn eviction_must_not_wake_under_lock() {
    let ps = Arc::new(PollSet::new());

    // Slot 0 is the reentrant waker; the remaining 63 slots are dummies.
    let reentrant = Arc::new(ReentrantWaker { ps: ps.clone() });
    let reentrant_waker = Waker::from(reentrant);
    let dummy = Waker::noop();

    for i in 0..64 {
        if i == 0 {
            ps.register(&reentrant_waker);
        } else {
            ps.register(dummy);
        }
    }

    // The 65th registration evicts slot 0 and wakes it. If `register` wakes the
    // evicted waker while holding the lock, the reentrant waker deadlocks and
    // this thread never sends on the channel.
    let (tx, rx) = mpsc::channel();
    let ps = ps.clone();
    let handle = thread::spawn(move || {
        ps.register(dummy);
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
