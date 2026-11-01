use std::{task::Waker, time::Instant};

use axpoll::PollSet;

/// Runs `f` `iters` times per sample, returns the best (minimum) ns/op over
/// `samples` samples. Taking the minimum reduces timer/OS noise.
fn best_of(mut f: impl FnMut(), iters: usize, samples: usize) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iters {
            f();
        }
        let ns = start.elapsed().as_nanos() as f64 / iters as f64;
        best = best.min(ns);
    }
    best
}

fn main() {
    let waker = Waker::noop();

    // Steady-state `register` on a full set.
    let ps = PollSet::new();
    for _ in 0..64 {
        ps.register(waker);
    }
    let register = best_of(|| ps.register(waker), 1_000_000, 5);

    // A single register followed by wake(). With only one waker the register
    // cost is tiny, so this number is dominated by wake() -- the target of the
    // allocation optimization.
    let ps = PollSet::new();
    let cycle1 = best_of(
        || {
            ps.register(waker);
            let _ = ps.wake();
        },
        200_000,
        5,
    );

    // Full drain: 64 registers + one wake().
    let ps = PollSet::new();
    let cycle64 = best_of(
        || {
            for _ in 0..64 {
                ps.register(waker);
            }
            let _ = ps.wake();
        },
        100_000,
        5,
    );

    println!("register (full set)  {:8.1} ns/op", register);
    println!("1 reg + wake()       {:8.1} ns/op", cycle1);
    println!("64 reg + wake()      {:8.1} ns/op", cycle64);
}
