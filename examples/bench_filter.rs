use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Wake, Waker},
    time::Instant,
};

use axpoll::{IoEvents, PollSet};

/// A waker that simulates the non-trivial cost of waking/scheduling a real
/// task: a few atomic operations (a real task wake takes scheduler locks).
struct BusyWaker(AtomicUsize);

impl Wake for BusyWaker {
    fn wake(self: Arc<Self>) {
        for _ in 0..8 {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn wake_by_ref(self: &Arc<Self>) {
        for _ in 0..8 {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn busy() -> Waker {
    Waker::from(Arc::new(BusyWaker(AtomicUsize::new(0))))
}

/// Times a single wake delivery (the set is refilled outside the timed region),
/// returning the minimum ns over `samples` deliveries.
fn time_wake<F: FnMut()>(mut fill: F, wake: impl Fn() -> usize, samples: usize) -> f64 {
    let mut best = u128::MAX;
    for _ in 0..samples {
        fill();
        let t = Instant::now();
        let _ = wake();
        best = best.min(t.elapsed().as_nanos());
    }
    best as f64
}

fn main() {
    // (a) Unfiltered: 64 tasks all wait for IN; wake() wakes all 64.
    let ps = PollSet::new();
    let all: Vec<Waker> = (0..64).map(|_| busy()).collect();
    let wake_all = time_wake(
        || {
            for w in &all {
                ps.register(w);
            }
        },
        || ps.wake(),
        20_000,
    );

    // (b) Filtered: 63 tasks wait for OUT (persistent), 1 waits for IN.
    //     wake_events(IN) wakes only the 1 IN task.
    let ps = PollSet::new();
    let out: Vec<Waker> = (0..63).map(|_| busy()).collect();
    for w in &out {
        ps.register_with_events(w, IoEvents::OUT);
    }
    let one = busy();
    let wake_one = time_wake(
        || ps.register_with_events(&one, IoEvents::IN),
        || ps.wake_events(IoEvents::IN),
        20_000,
    );

    println!("wake()         wakes 64 tasks : {:8.1} ns", wake_all);
    println!("wake_events()  wakes 1 of 64  : {:8.1} ns", wake_one);
    println!(
        "fewer wake deliveries => {:.1}x cheaper",
        wake_all / wake_one
    );
}
