#![feature(new_uninit, maybe_uninit_ref, maybe_uninit_slice_assume_init)]

#[cfg(not(feature = "loom"))]
mod loom;

mod waker;
mod sync;

#[macro_use]
pub mod util;
pub mod handle;
pub mod action;
pub mod executor;

use std::{ io, ptr, mem };
use std::sync::Arc;
use std::cell::RefCell;
use std::time::Duration;
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use futures_task::{ self as task, WakerRef, Waker };
use static_assertions::const_assert_eq;
use io_uring::opcode::{ self, types };
use io_uring::{ squeue, cqueue, IoUring };
use crate::waker::EventFd;
pub use crate::sync::{ Ticket, TicketFuture };


pub type SubmissionEntry = squeue::Entry;
pub type CompletionEntry = cqueue::Entry;

const WAKE_TOKEN: u64 = 0x0;

pub struct Proactor {
    ring: Rc<RefCell<IoUring>>,
    eventfd: Arc<EventFd>,
    eventbuf: mem::ManuallyDrop<Box<[u8; 8]>>,
    timeout: Box<types::Timespec>,
}

#[derive(Clone)]
pub struct RawHandle {
    ring: Rc<RefCell<IoUring>>,
}

impl Proactor {
    pub fn new() -> io::Result<Proactor> {
        let ring = io_uring::IoUring::new(256)?; // TODO better number

        Ok(Proactor {
            ring: Rc::new(RefCell::new(ring)),
            eventfd: Arc::new(EventFd::new()?),
            eventbuf: mem::ManuallyDrop::new(Box::new([0; 8])), // TODO not leak it :(
            timeout: Box::new(types::Timespec::default())
        })
    }

    pub fn waker(&self) -> Waker {
        task::waker(self.eventfd.clone())
    }

    pub fn waker_ref(&self) -> WakerRef {
        task::waker_ref(&self.eventfd)
    }

    pub fn raw_handle(&self) -> RawHandle {
        RawHandle {
            ring: Rc::clone(&self.ring)
        }
    }

    pub fn park(&mut self, dur: Option<Duration>) -> io::Result<()> {
        let mut ring = self.ring.borrow_mut();
        let (submitter, sq, cq) = ring.split();
        let (mut sq, mut cq) = (sq.available(), cq.available());
        let cq_is_not_empty = cq.len() != 0;

        // clean cq
        cq_drain(&mut cq);

        let state = self.eventfd.park();

        // we has events, so we don't need to wait for timeout
        let nowait = state.is_ready()
            || cq_is_not_empty
            || dur == Some(Duration::from_secs(0));

        let mut event_e = if !state.is_park() {
            let op = types::Target::Fd(self.eventfd.as_raw_fd());
            let bufptr = self.eventbuf.as_mut_ptr();
            let entry = opcode::Read::new(op, bufptr, 8)
                .build()
                .user_data(WAKE_TOKEN);
            Some(entry)
        } else {
            None
        };

        let mut timeout_e = if let Some(dur) = dur.filter(|_| !nowait) {
            self.timeout.tv_sec = dur.as_secs() as _;
            self.timeout.tv_nsec = dur.subsec_nanos() as _;
            let entry = opcode::Timeout::new(&*self.timeout)
                .build()
                .user_data(WAKE_TOKEN);
            Some(entry)
        } else {
            None
        };

        const_assert_eq!(false as usize , 0);
        const_assert_eq!(true as usize , 1);

        let n = event_e.is_some() as usize + timeout_e.is_some() as usize;
        if sq.capacity() - sq.len() < n {
            submitter.submit()?;
        }

        unsafe {
            if let Some(entry) = event_e.take() {
                sq.push(entry).ok().unwrap();
            }

            if let Some(entry) = timeout_e.take() {
                sq.push(entry).ok().unwrap();
            }
        }

        if nowait {
            submitter.submit()?;
        } else {
            submitter.submit_and_wait(1)?;
        }

        cq.sync();

        cq_drain(&mut cq);

        // reset eventfd
        self.eventfd.reset();

        Ok(())
    }
}

fn cq_drain(cq: &mut cqueue::AvailableQueue) {
    for entry in cq {
        match entry.user_data() {
            WAKE_TOKEN => (),
            ptr => unsafe {
                Ticket::from_raw(ptr::NonNull::new_unchecked(ptr as _))
                    .send(entry.clone());
            }
        }
    }
}

impl RawHandle {
    pub unsafe fn raw_push(&self, mut entry: SubmissionEntry) -> io::Result<()> {
        let mut ring = self.ring.borrow_mut();
        let (submitter, sq, cq) = ring.split();

        loop {
            let mut sq = sq.available();

            match sq.push(entry) {
                Ok(_) => break,
                Err(e) => entry = e
            }

            match submitter.submit() {
                Ok(_) => (),
                Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => {
                    cq_drain(&mut cq.available());
                    submitter.submit()?;
                },
                Err(err) => return Err(err)
            }
        }

        Ok(())
    }

    fn into_raw(self) -> *const RawHandle {
        Rc::into_raw(self.ring) as *const _
    }

    unsafe fn from_raw(ptr: *const RawHandle) -> RawHandle {
        RawHandle {
            ring: Rc::from_raw(ptr as *const _)
        }
    }
}
