#[cfg(test)]
extern crate std;

#[cfg(test)]
mod test {
    use super::*;
    use crate::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::{Context, RawWaker, RawWakerVTable, Waker};

    fn make_waker(counter: Arc<AtomicUsize>) -> Waker {
        unsafe fn clone(ptr: *const ()) -> RawWaker {
            let arc = unsafe { Arc::<AtomicUsize>::from_raw(ptr as *const AtomicUsize) };
            let arc2 = arc.clone();
            std::mem::forget(arc);
            RawWaker::new(Arc::into_raw(arc2) as *const (), &VTABLE)
        }
        unsafe fn wake(ptr: *const ()) {
            let arc = unsafe { Arc::<AtomicUsize>::from_raw(ptr as *const AtomicUsize) };
            arc.fetch_add(1, Ordering::SeqCst);
        }
        unsafe fn wake_by_ref(ptr: *const ()) {
            let arc = unsafe { Arc::<AtomicUsize>::from_raw(ptr as *const AtomicUsize) };
            arc.fetch_add(1, Ordering::SeqCst);
            std::mem::forget(arc);
        }
        unsafe fn drop_arc(ptr: *const ()) {
            let _ = unsafe { Arc::<AtomicUsize>::from_raw(ptr as *const AtomicUsize) };
        }

        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop_arc);

        let raw = RawWaker::new(Arc::into_raw(counter.clone()) as *const (), &VTABLE);
        unsafe { Waker::from_raw(raw) }
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
