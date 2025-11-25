use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Wake, Waker},
};

use axpoll::PollSet;

struct Counter(Arc<AtomicUsize>);

impl Counter {
    fn new() -> Self {
        Self(Arc::new(AtomicUsize::new(0)))
    }

    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    fn add(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.add();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.add();
    }
}

fn make_waker(counter: Arc<AtomicUsize>) -> Waker {
    let wrapped = Arc::new(Counter(counter.clone()));
    Waker::from(wrapped)
}

#[test]
fn register_and_wake() {
    let ps = PollSet::new();
    let counter = Counter::new();
    let w = make_waker(counter.0.clone());
    ps.register(&w);
    assert_eq!(ps.wake(), 1);
    assert_eq!(counter.count(), 1);
}

#[test]
fn empty_return() {
    let ps = PollSet::new();
    assert_eq!(ps.wake(), 0);
}

#[test]
fn full_capacity() {
    let ps = PollSet::new();
    let counter = Counter::new();
    for _ in 0..64 {
        let w = make_waker(counter.0.clone());
        let cx = Context::from_waker(&w);
        ps.register(cx.waker());
    }
    let woke = ps.wake();
    assert_eq!(woke, 64);
    assert_eq!(counter.count(), 64);
}

#[test]
fn overwrite() {
    let ps = PollSet::new();
    let counters = (0..65).map(|_| Counter::new()).collect::<Vec<_>>();
    for c in &counters {
        let w = make_waker(c.0.clone());
        let cx = Context::from_waker(&w);
        ps.register(cx.waker());
    }
    assert_eq!(ps.wake(), 64);
    let total: usize = counters.iter().map(|c| c.count()).sum();
    assert_eq!(total, 65);
}

#[test]
fn drop_wakes() {
    let ps = PollSet::new();
    let counters = Counter::new();
    for _ in 0..10 {
        let w = make_waker(counters.0.clone());
        let cx = Context::from_waker(&w);
        ps.register(cx.waker());
    }
    drop(ps);
    assert_eq!(counters.count(), 10);
}
