use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    task::{Context, Wake, Waker},
    thread,
    time::Duration,
};

use rand::{Rng, rng};
use axpoll::PollSet;

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
    let counter = Counter::new();
    for _ in 0..64 {
        let w = Waker::from(counter.clone());
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
    let counters = Counter::new();
    for _ in 0..10 {
        let w = Waker::from(counters.clone());
        let cx = Context::from_waker(&w);
        ps.register(cx.waker());
    }
    drop(ps);
    assert_eq!(counters.count(), 10);
}

#[test]
fn concurrent_registers() {
    let ps = Arc::new(PollSet::new());
    let (tx, rx) = mpsc::channel();
    let threads_n = 50usize;
    let per_thread = 200usize;
    let total = threads_n * per_thread;
    let mut handles = Vec::new();

    let ps1 = ps.clone();
    let wake_handle = thread::spawn(move || {
        for _ in 0..(threads_n * 10) {
            ps1.as_ref().wake();
            let mut rng = rng();
            let s = rng.random_range(0..3);
            thread::sleep(Duration::from_millis(s));
        }
    });
    for _ in 0..threads_n {
        let tx = tx.clone();
        let ps = ps.clone();
        let handle = thread::spawn(move || {
            let mut rng = rng();
            for _ in 0..per_thread {
                let counter = Counter::new();
                let w = Waker::from(counter.clone());
                let cx = Context::from_waker(&w);
                if rng.random_bool(0.1) {
                    thread::sleep(Duration::from_micros(rng.random_range(0..500)));
                }
                ps.register(cx.waker());
                tx.send(counter).unwrap();
            }
        });
        handles.push(handle);
    }

    drop(tx);
    for h in handles {
        h.join().unwrap();
    }
    wake_handle.join().unwrap();
    let counters: Vec<_> = rx.into_iter().collect();
    ps.wake();
    let woke: usize = counters.iter().map(|c| c.count()).sum();
    assert_eq!(counters.len(), total);
    assert_eq!(woke, total);
}
