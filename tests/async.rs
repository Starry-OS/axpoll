use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use axpoll::PollSet;
use tokio::{task, sync::Barrier};

struct WaitFuture {
    ps: Arc<PollSet>,
    ready: Arc<AtomicBool>,
}

impl Future for WaitFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready.load(Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            self.ps.register(cx.waker());
            Poll::Pending
        }
    }
}

impl WaitFuture {
    fn new(ps: Arc<PollSet>, ready: Arc<AtomicBool>) -> Self {
        Self { ps, ready }
    }
}

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

#[tokio::test]
async fn async_wake_single() {
    let ps = Arc::new(PollSet::new());
    let ready = Arc::new(AtomicBool::new(false));

    let f = WaitFuture::new(ps.clone(), ready.clone());

    let handle = tokio::spawn(async move {
        ready.store(true, Ordering::SeqCst);
        ps.wake();
    });

    f.await;
    handle.await.unwrap();
}

#[tokio::test]
async fn async_wake_many() {
    let ps = Arc::new(PollSet::new());
    let counter = Counter::new();

    let mut flags = Vec::new();
    let mut handles = Vec::new();
    let barrier = Arc::new(Barrier::new(66));
    for i in 0..65 {
        let flag = Arc::new(AtomicBool::new(false));
        if i % 2 == 0 {
            flag.store(true, Ordering::SeqCst);
        }
        let b = barrier.clone();
        let f = WaitFuture::new(ps.clone(), flag.clone());
        let counter = counter.clone();
        let h = tokio::spawn(async move {
            b.wait().await;
            f.await;
            counter.add();
        });
        flags.push(flag);
        handles.push(h);
    }
    barrier.wait().await;
    let pending: Vec<_> = flags.iter().filter(|f| !f.load(Ordering::SeqCst)).collect();

    ps.wake();
    task::yield_now().await;
    assert_eq!(counter.count(), 33);
    
    for f in &pending {
        f.store(true, Ordering::SeqCst);
    }
    ps.wake();
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(counter.count(), 65);
}
