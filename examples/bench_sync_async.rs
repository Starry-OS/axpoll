use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Wake, Waker},
    time::Instant,
};

use axpoll::PollSet;

/// A waker that does a small amount of "real work" on wake (an atomic bump),
/// to simulate the cost of waking/scheduling a real task.
struct BusyWaker(AtomicUsize);

impl Wake for BusyWaker {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

/// Runs `f` `iters` times per sample, returns the best (minimum) ns/op over
/// `samples` samples, to reduce timer/OS noise.
fn best_of(mut f: impl FnMut(), iters: usize, samples: usize) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iters {
            f();
        }
        best = best.min(start.elapsed().as_nanos() as f64 / iters as f64);
    }
    best
}

fn main() {
    let one = Waker::from(Arc::new(BusyWaker(AtomicUsize::new(0))));

    // 1. Per-task delivery cost:
    //    - sync: the producer calls `waker.wake()` inline.
    //    - async: the producer only pushes the waker to a queue; a later
    //      executor drain does the actual `wake()`.
    let wake_one = best_of(|| one.wake_by_ref(), 2_000_000, 7);

    let mut queue = VecDeque::new();
    let enqueue_one = best_of(|| queue.push_back(one.clone()), 2_000_000, 7);

    println!("=== per-task delivery cost ===");
    println!("sync   wake():    {:7.1} ns/task", wake_one);
    println!("async  enqueue:   {:7.1} ns/task", enqueue_one);
    if enqueue_one > wake_one {
        println!(
            "=> deferring is NOT cheaper: enqueue is {:.1}x MORE expensive than wake",
            enqueue_one / wake_one
        );
    } else {
        println!("=> deferring is {:.1}x cheaper per task", wake_one / enqueue_one);
    }

    // 2. Scaling: what the producer pays to deliver readiness to N tasks.
    //    "sync" = current PollSet::wake() (wakes all N inline).
    //    "async est" = N * enqueue_one (producer only enqueues).
    println!();
    println!("=== producer cost to deliver readiness to N tasks ===");
    println!(
        "{:>4} | {:>16} | {:>18}",
        "N", "sync (wake)", "async est (enqueue)"
    );

    for &n in &[1usize, 4, 16, 64] {
        let wakers: Vec<Waker> = (0..n)
            .map(|_| Waker::from(Arc::new(BusyWaker(AtomicUsize::new(0)))))
            .collect();

        let ps = PollSet::new();
        for w in &wakers {
            ps.register(w);
        }
        let sync = best_of(
            || {
                for w in &wakers {
                    ps.register(w);
                }
                let _ = ps.wake();
            },
            20_000,
            7,
        );

        println!("{:>4} | {:>16.1} | {:>18.1}", n, sync, enqueue_one * n as f64);
    }
}
