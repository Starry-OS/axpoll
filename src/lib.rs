//! A library for polling I/O events and waking up tasks.

#![no_std]
#![deny(missing_docs)]

extern crate alloc;

use core::{
    mem::MaybeUninit,
    task::{Context, Waker},
};

use alloc::{boxed::Box, sync::Arc, task::Wake};
use bitflags::bitflags;
use linux_raw_sys::general::*;
use spin::{Lazy, Mutex};

bitflags! {
    /// I/O events.
    #[derive(Debug, Clone, Copy)]
    pub struct IoEvents: u32 {
        /// Available for read
        const IN     = POLLIN;
        /// Urgent data for read
        const PRI    = POLLPRI;
        /// Available for write
        const OUT    = POLLOUT;

        /// Error condition
        const ERR    = POLLERR;
        /// Hang up
        const HUP    = POLLHUP;
        /// Invalid request
        const NVAL   = POLLNVAL;

        /// Equivalent to [`IN`](Self::IN)
        const RDNORM = POLLRDNORM;
        /// Priority band data can be read
        const RDBAND = POLLRDBAND;
        /// Equivalent to [`OUT`](Self::OUT)
        const WRNORM = POLLWRNORM;
        /// Priority data can be written
        const WRBAND = POLLWRBAND;

        /// Message
        const MSG    = POLLMSG;
        /// Remove
        const REMOVE = POLLREMOVE;
        /// Stream socket peer closed connection, or shut down writing half of connection.
        const RDHUP  = POLLRDHUP;

        /// Events that are always polled even without specifying them.
        const ALWAYS_POLL = Self::ERR.bits() | Self::HUP.bits();
    }
}

/// Trait for types that can be polled for I/O events.
pub trait Pollable {
    /// Polls for I/O events.
    fn poll(&self) -> IoEvents;

    /// Registers wakers for I/O events.
    fn register(&self, context: &mut Context<'_>, events: IoEvents);
}

const POLL_SET_CAPACITY: usize = 64;

/// A single waiting task: its waker and the events it is interested in.
struct Entry {
    waker: Waker,
    events: IoEvents,
}

struct Inner {
    entries: Box<[MaybeUninit<Entry>]>,
    cursor: usize,
}

impl Inner {
    fn new() -> Self {
        Self {
            entries: Box::new_uninit_slice(POLL_SET_CAPACITY),
            cursor: 0,
        }
    }

    fn len(&self) -> usize {
        self.cursor.min(POLL_SET_CAPACITY)
    }

    fn is_empty(&self) -> bool {
        self.cursor == 0
    }

    fn register(&mut self, waker: &Waker, events: IoEvents) -> Option<Waker> {
        let slot = self.cursor % POLL_SET_CAPACITY;
        let evicted = if self.cursor >= POLL_SET_CAPACITY {
            let old = unsafe { self.entries[slot].assume_init_read() };
            self.cursor = ((slot + 1) % POLL_SET_CAPACITY) + POLL_SET_CAPACITY;
            (!old.waker.will_wake(waker)).then_some(old.waker)
        } else {
            self.cursor += 1;
            None
        };
        self.entries[slot].write(Entry {
            waker: waker.clone(),
            events,
        });
        evicted
    }

    /// Moves every entry whose events match `filter` out into `buf`, keeps the
    /// non-matching entries (compacted in place), and returns the number moved.
    ///
    /// This keeps the backing buffer alive, so it performs no allocation.
    #[allow(clippy::needless_range_loop)]
    fn take_where(
        &mut self,
        buf: &mut [MaybeUninit<Entry>; POLL_SET_CAPACITY],
        filter: impl Fn(IoEvents) -> bool,
    ) -> usize {
        let n = self.len();
        let mut out = 0;
        let mut kept = 0;
        for i in 0..n {
            let entry = unsafe { self.entries[i].assume_init_read() };
            if filter(entry.events) {
                buf[out].write(entry);
                out += 1;
            } else {
                self.entries[kept].write(entry);
                kept += 1;
            }
        }
        self.cursor = kept;
        out
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        for i in 0..self.len() {
            unsafe { self.entries[i].assume_init_read() }.waker.wake();
        }
    }
}

/// A data structure for waking up tasks that are waiting for I/O events.
pub struct PollSet(Lazy<Mutex<Inner>>);

impl Default for PollSet {
    fn default() -> Self {
        Self::new()
    }
}

impl PollSet {
    /// Creates a new empty [`PollSet`].
    pub const fn new() -> Self {
        Self(Lazy::new(|| Mutex::new(Inner::new())))
    }

    /// Registers a waker interested in all events.
    pub fn register(&self, waker: &Waker) {
        self.register_with_events(waker, IoEvents::all());
    }

    /// Registers a waker interested only in `events`.
    pub fn register_with_events(&self, waker: &Waker, events: IoEvents) {
        let evicted = self.0.lock().register(waker, events);
        if let Some(evicted) = evicted {
            evicted.wake();
        }
    }

    /// Wakes up all registered wakers, and returns the number woken.
    pub fn wake(&self) -> usize {
        self.drain(|_| true)
    }

    /// Wakes up only the wakers whose registered events intersect `events`,
    /// and returns the number woken.
    pub fn wake_events(&self, events: IoEvents) -> usize {
        self.drain(|e| e.intersects(events))
    }

    /// Drains (and wakes) every entry matching `filter`, outside the lock.
    fn drain(&self, filter: impl Fn(IoEvents) -> bool) -> usize {
        let mut buf: [MaybeUninit<Entry>; POLL_SET_CAPACITY] =
            core::array::from_fn(|_| MaybeUninit::uninit());
        let n = {
            let mut guard = self.0.lock();
            if guard.is_empty() {
                return 0;
            }
            guard.take_where(&mut buf, filter)
        };
        for entry in &mut buf[..n] {
            let entry = unsafe { core::mem::replace(entry, MaybeUninit::uninit()).assume_init() };
            entry.waker.wake();
        }
        n
    }
}

impl Drop for PollSet {
    fn drop(&mut self) {
        // Ensure all entries are dropped (wake everyone, regardless of events).
        self.drain(|_| true);
    }
}

impl Wake for PollSet {
    fn wake(self: Arc<Self>) {
        self.as_ref().drain(|_| true);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.as_ref().drain(|_| true);
    }
}
