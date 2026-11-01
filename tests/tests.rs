use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Wake, Waker},
};

use axpoll::{IoEvents, PollSet};

struct Counter(AtomicUsize);

impl Counter {
    fn new() -> Arc<Self> {
        Arc::new(Self(AtomicUsize::new(0)))
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

#[test]
fn register_and_wake() {
    let ps = PollSet::new();
    let counter = Counter::new();
    let w = Waker::from(counter.clone());
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
    let counters = (0..64).map(|_| Counter::new()).collect::<Vec<_>>();
    for c in &counters {
        let w = Waker::from(c.clone());
        ps.register(&w);
    }
    let woke = ps.wake();
    assert_eq!(woke, 64);
    for c in &counters {
        assert_eq!(c.count(), 1);
    }
}

#[test]
fn overwrite() {
    let ps = PollSet::new();
    let counters = (0..65).map(|_| Counter::new()).collect::<Vec<_>>();
    for c in &counters {
        let w = Waker::from(c.clone());
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
    let counters = (0..10).map(|_| Counter::new()).collect::<Vec<_>>();
    for c in &counters {
        let w = Waker::from(c.clone());
        ps.register(&w);
    }
    drop(ps);
    for c in &counters {
        assert_eq!(c.count(), 1);
    }
}

#[test]
fn register_merges_events() {
    let ps = PollSet::new();
    let counter = Counter::new();
    let waker = Waker::from(counter.clone());

    ps.register_with_events(&waker, IoEvents::IN);
    ps.register_with_events(&waker, IoEvents::OUT);

    // Deduplicated into a single entry (IN|OUT), woken exactly once.
    assert_eq!(ps.wake(), 1);
    assert_eq!(counter.count(), 1);
}

#[test]
fn same_waker_not_duplicated() {
    let ps = PollSet::new();
    let counter = Counter::new();
    let waker = Waker::from(counter.clone());
    for _ in 0..65 {
        ps.register(&waker);
    }
    // Deduplicated into a single entry, so it is woken exactly once.
    assert_eq!(ps.wake(), 1);
    assert_eq!(counter.count(), 1);
}

#[test]
fn many_distinct_wakers() {
    let ps = PollSet::new();
    let counters = (0..200).map(|_| Counter::new()).collect::<Vec<_>>();
    for c in &counters {
        let waker = Waker::from(c.clone());
        ps.register(&waker);
    }
    assert_eq!(ps.wake(), 64);
    let total: usize = counters.iter().map(|c| c.count()).sum();
    assert_eq!(total, 200);
}

#[test]
fn pollset_as_waker() {
    let ps = Arc::new(PollSet::new());
    let counter = Counter::new();
    let task_waker = Waker::from(counter.clone());
    ps.register(&task_waker);

    let ps_waker = Waker::from(ps.clone());
    ps_waker.wake();

    assert_eq!(counter.count(), 1);
}

#[test]
fn io_events_flags() {
    assert_eq!(
        IoEvents::ALWAYS_POLL.bits(),
        (IoEvents::ERR | IoEvents::HUP).bits()
    );
    let events = IoEvents::IN | IoEvents::OUT;
    assert!(events.contains(IoEvents::IN));
    assert!(events.contains(IoEvents::OUT));
    assert!(!events.contains(IoEvents::ERR));
    assert!(IoEvents::ALWAYS_POLL.contains(IoEvents::ERR));
}

#[test]
fn partial_fill() {
    let ps = PollSet::new();
    let counters = (0..3).map(|_| Counter::new()).collect::<Vec<_>>();
    for c in &counters {
        let waker = Waker::from(c.clone());
        ps.register(&waker);
    }
    assert_eq!(ps.wake(), 3);
    for c in &counters {
        assert_eq!(c.count(), 1);
    }
}

#[test]
fn concurrent_register_and_wake() {
    let ps = Arc::new(PollSet::new());
    let mut register_handles = Vec::new();
    for _ in 0..8 {
        let ps = ps.clone();
        register_handles.push(std::thread::spawn(move || {
            let counter = Counter::new();
            let waker = Waker::from(counter);
            for _ in 0..1000 {
                ps.register(&waker);
            }
        }));
    }

    let ps = ps.clone();
    let wake_handle = std::thread::spawn(move || {
        for _ in 0..1000 {
            let _ = PollSet::wake(&ps);
        }
    });

    for h in register_handles {
        h.join().unwrap();
    }
    wake_handle.join().unwrap();
}

#[test]
fn wake_events_filters_by_events() {
    let ps = PollSet::new();
    let read = Counter::new();
    let write = Counter::new();

    ps.register_with_events(&Waker::from(read.clone()), IoEvents::IN);
    ps.register_with_events(&Waker::from(write.clone()), IoEvents::OUT);

    // Wake only readers.
    assert_eq!(ps.wake_events(IoEvents::IN), 1);
    assert_eq!(read.count(), 1);
    assert_eq!(write.count(), 0);

    // The writer is still waiting; wake it with OUT.
    assert_eq!(ps.wake_events(IoEvents::OUT), 1);
    assert_eq!(write.count(), 1);
    assert_eq!(read.count(), 1);

    // Nothing left.
    assert_eq!(ps.wake_events(IoEvents::IN | IoEvents::OUT), 0);
}

#[test]
fn wake_events_wakes_fewer() {
    let ps = PollSet::new();
    let readers: Vec<_> = (0..4).map(|_| Counter::new()).collect();
    let writers: Vec<_> = (0..4).map(|_| Counter::new()).collect();

    for c in &readers {
        ps.register_with_events(&Waker::from(c.clone()), IoEvents::IN);
    }
    for c in &writers {
        ps.register_with_events(&Waker::from(c.clone()), IoEvents::OUT);
    }

    // Only the 4 readers are woken; the 4 writers are NOT (no spurious wakeup).
    assert_eq!(ps.wake_events(IoEvents::IN), 4);
    assert!(readers.iter().all(|c| c.count() == 1));
    assert!(writers.iter().all(|c| c.count() == 0));

    // Writers are still waiting and get woken later.
    assert_eq!(ps.wake_events(IoEvents::OUT), 4);
    assert!(writers.iter().all(|c| c.count() == 1));
}
