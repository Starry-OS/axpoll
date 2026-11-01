use std::task::Waker;

use axpoll::PollSet;
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_register(c: &mut Criterion) {
    let ps = PollSet::new();
    let waker = Waker::noop();
    for _ in 0..64 {
        ps.register(waker);
    }
    c.bench_function("register", |b| b.iter(|| ps.register(waker)));
}

fn bench_wake_cycle(c: &mut Criterion) {
    c.bench_function("64 reg + wake", |b| {
        let ps = PollSet::new();
        let waker = Waker::noop();
        b.iter(|| {
            for _ in 0..64 {
                ps.register(waker);
            }
            ps.wake();
        });
    });
}

criterion_group!(benches, bench_register, bench_wake_cycle);
criterion_main!(benches);
