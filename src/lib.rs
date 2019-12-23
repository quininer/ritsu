#![feature(weak_into_raw)]

mod waker;
pub mod oneshot;
pub mod action;
pub mod executor;

use std::{ io, mem };
use std::sync::Arc;
use std::cell::RefCell;
use std::time::Duration;
use std::rc::{ Rc, Weak };
use std::marker::PhantomData;
use std::os::unix::io::AsRawFd;
use futures_task::{ self as task, WakerRef, Waker };
use io_uring::{ squeue, cqueue, opcode, IoUring };
use crate::waker::EventFd;


pub type SubmissionEntry = squeue::Entry;
pub type CompletionEntry = cqueue::Entry;
pub type LocalHandle = Handle<oneshot::Sender<CompletionEntry>>;

const EVENT_EMPTY: [u8; 8] = [0; 8];
const EVENT_TOKEN: u64 = 0x00;
const TIMEOUT_TOKEN: u64 = 0x00u64.wrapping_sub(1);

pub struct Proactor<C: Ticket> {
    ring: Rc<RefCell<IoUring>>,
    eventfd: Arc<EventFd>,
    eventbuf: Box<[u8; 8]>,
    eventbufs: Box<[libc::iovec; 1]>,
    timeout: Box<opcode::Timespec>,
    _mark: PhantomData<C>
}

#[derive(Clone)]
pub struct Handle<C: Ticket> {
    ring: Weak<RefCell<IoUring>>,
    _mark: PhantomData<C>
}

pub trait Ticket {
    fn into_raw(self) -> *const ();
    unsafe fn from_raw(ptr: *const ()) -> Self;

    fn set(self, item: CompletionEntry);
}

impl<C: Ticket> Proactor<C> {
    pub fn new() -> io::Result<Proactor<C>> {
        let ring = io_uring::IoUring::new(256)?; // TODO better number
        let mut eventbuf = Box::new([0; 8]);
        let eventbuf_ptr =
            unsafe { mem::transmute::<_, libc::iovec>(io::IoSliceMut::new(&mut *eventbuf)) };
        let eventbufs = Box::new([eventbuf_ptr]);

        Ok(Proactor {
            ring: Rc::new(RefCell::new(ring)),
            eventfd: Arc::new(EventFd::new()?),
            eventbuf, eventbufs,
            timeout: Box::new(opcode::Timespec::default()),
            _mark: PhantomData
        })
    }

    pub fn waker(&self) -> Waker {
        task::waker(self.eventfd.clone())
    }

    pub fn waker_ref(&self) -> WakerRef {
        task::waker_ref(&self.eventfd)
    }

    pub fn handle(&self) -> Handle<C> {
        Handle {
            ring: Rc::downgrade(&self.ring),
            _mark: PhantomData
        }
    }

    fn maybe_event(&mut self) -> Option<squeue::Entry> {
        if EVENT_EMPTY == *self.eventbuf {
            return None;
        }

        self.eventbuf.copy_from_slice(&EVENT_EMPTY);

        let op = opcode::Target::Fd(self.eventfd.as_raw_fd());
        let bufs_ptr = self.eventbufs.as_mut_ptr();

        let entry = opcode::Readv::new(op, bufs_ptr, 1)
            .build()
            .user_data(EVENT_TOKEN);
        Some(entry)
    }

    /// TODO
    ///
    /// The current timeout implement may cause spurious wakeups.
    fn maybe_timeout(&mut self, dur: Duration) -> Option<squeue::Entry> {
        if dur == Duration::from_secs(0) {
            return None;
        }

        self.timeout.tv_sec = dur.as_secs() as _;
        self.timeout.tv_nsec = dur.subsec_nanos() as _;

        let entry = opcode::Timeout::new(&*self.timeout)
            .build()
            .user_data(TIMEOUT_TOKEN);
        Some(entry)
    }

    pub fn park(&mut self, dur: Option<Duration>) -> io::Result<()> {
        let mut event_e = self.maybe_event();
        let mut timeout_e = if let Some(dur) = dur {
            self.maybe_timeout(dur)
        } else {
            None
        };
        let nowait = dur.is_some() && timeout_e.is_none();

        let mut ring = self.ring.borrow_mut();
        let (submitter, sq, cq) = ring.split();

        while event_e.is_some() || timeout_e.is_some() {
            let mut sq = sq.available();

            unsafe {
                if let Some(entry) = event_e.take() {
                    if let Err(entry) = sq.push(entry) {
                        event_e = Some(entry);
                    }
                }

                if let Some(entry) = timeout_e.take() {
                    if let Err(entry) = sq.push(entry) {
                        timeout_e = Some(entry);
                    }
                }
            }

            if event_e.is_some() || timeout_e.is_some() {
                submitter.submit()?;
            }
        }

        if nowait {
            submitter.submit()?;
        } else {
            submitter.submit_and_wait(1)?;
        }

        for entry in cq.available() {
            match entry.user_data() {
                EVENT_TOKEN | TIMEOUT_TOKEN => (),
                ptr => unsafe {
                    C::from_raw(ptr as _).set(entry.clone());
                }
            }
        }

        Ok(())
    }
}

impl<C: Ticket> Handle<C> {
    pub unsafe fn push(&self, sender: C, entry: SubmissionEntry) -> io::Result<()> {
        let ring = self.ring.upgrade()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "Proactor closed"))?;
        let mut ring = ring.borrow_mut();
        let mut entry = entry.user_data(sender.into_raw() as _);

        loop {
            match ring.submission().available().push(entry) {
                Ok(_) => break,
                Err(e) => entry = e
            }

            ring.submit()?;
        }

        Ok(())
    }
}
