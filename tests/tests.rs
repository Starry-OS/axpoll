#[cfg(test)]
extern crate std;

#[cfg(test)]
mod test {
    use super::*;
    use axpoll::PollSet;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::{Context, Wake, Waker};

    struct WrapArc(Arc<AtomicUsize>);

    impl Wake for WrapArc {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn make_waker(counter: Arc<AtomicUsize>) -> Waker {
        let wrapped = Arc::new(WrapArc(counter.clone()));
        Waker::from(wrapped)
    }

    #[test]
    fn register_and_wake() {
        let ps = PollSet::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let w = make_waker(counter.clone());
        ps.register(&w);
        assert_eq!(ps.wake(), 1);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn empty_return() {
        let ps = PollSet::new();
        assert_eq!(ps.wake(), 0);
    }

    #[test]
    fn full_capacity() {
        let ps = PollSet::new();
        let counter = Arc::new(AtomicUsize::new(0));
        for _ in 0..64 {
            let w = make_waker(counter.clone());
            let cx = Context::from_waker(&w);
            ps.register(cx.waker());
        }
        let woke = ps.wake();
        assert_eq!(woke, 64);
        assert_eq!(counter.load(Ordering::SeqCst), 64);
    }

    #[test]
    fn overwrite() {
        let ps = PollSet::new();
        let counters = (0..65)
            .map(|_| Arc::new(AtomicUsize::new(0)))
            .collect::<std::vec::Vec<Arc<AtomicUsize>>>();
        for c in &counters {
            let w = make_waker(c.clone());
            let cx = Context::from_waker(&w);
            ps.register(cx.waker());
        }
        assert_eq!(ps.wake(), 64);
        let total: usize = counters.iter().map(|c| c.load(Ordering::SeqCst)).sum();
        assert_eq!(total, 65);
    }

    #[test]
    fn drop_wakes() {
        let ps = PollSet::new();
        let counters = Arc::new(AtomicUsize::new(0));
        for _ in 0..10 {
            let w = make_waker(counters.clone());
            let cx = Context::from_waker(&w);
            ps.register(cx.waker());
        }
        drop(ps);
        assert_eq!(counters.load(Ordering::SeqCst), 10);
    }
}
