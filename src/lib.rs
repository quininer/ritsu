#![feature(weak_into_raw)]

#[cfg(not(feature = "loom"))]
mod loom;

mod waker;

#[macro_use]
pub mod util;
pub mod sync;
pub mod action;
pub mod executor;

use std::{ io, ptr };
use std::sync::Arc;
use std::cell::RefCell;
use std::time::Duration;
use std::os::unix::io::AsRawFd;
use std::rc::{ Rc, Weak };
use futures_task::{ self as task, WakerRef, Waker };
use io_uring::opcode::{ self, types };
use io_uring::{ squeue, cqueue, IoUring };
use crate::waker::EventFd;
pub use crate::sync::{ Ticket, TicketFuture };

pub type SubmissionEntry = squeue::Entry;
pub type CompletionEntry = cqueue::Entry;

const EVENT_TOKEN: u64 = 0x00;
const TIMEOUT_TOKEN: u64 = 0x00u64.wrapping_sub(1);

pub struct Proactor {
    ring: Rc<RefCell<IoUring>>,
    eventfd: Arc<EventFd>,
    eventbuf: Box<[u8; 8]>,
    timeout: Box<types::Timespec>,
}

fn cq_drain(cq: &mut cqueue::AvailableQueue) {
    for entry in cq {
        match entry.user_data() {
            EVENT_TOKEN | TIMEOUT_TOKEN => (),
            ptr => unsafe {
                Ticket::from_raw(ptr::NonNull::new_unchecked(ptr as _))
                    .send(entry.clone());
            }
        }
    }
}

impl Proactor {
    pub fn new() -> io::Result<Proactor> {
        let ring = io_uring::IoUring::new(256)?; // TODO better number

        Ok(Proactor {
            ring: Rc::new(RefCell::new(ring)),
            eventfd: Arc::new(EventFd::new()?),
            eventbuf: Box::new([0; 8]),
            timeout: Box::new(types::Timespec::default())
        })
    }

    pub fn waker(&self) -> Waker {
        task::waker(self.eventfd.clone())
    }

    pub fn waker_ref(&self) -> WakerRef {
        task::waker_ref(&self.eventfd)
    }

    pub fn handle(&self) -> Handle {
        Handle {
            ring: Rc::downgrade(&self.ring)
        }
    }

    // TODO refactor it
    pub fn park(&mut self, dur: Option<Duration>) -> io::Result<()> {
        let mut ring = self.ring.borrow_mut();
        let (submitter, sq, cq) = ring.split();
        let (mut sq, mut cq) = (sq.available(), cq.available());
        let cq_is_not_empty = cq.len() != 0;

        // handle before eventfd to avoid unnecessary wakeup
        cq_drain(&mut cq);

        let mut event_e = if self.eventfd.get() {
            let op = types::Target::Fd(self.eventfd.as_raw_fd());
            let iovec_ptr = self.eventbuf.as_mut_ptr();
            let entry = opcode::Read::new(op, iovec_ptr, 8)
                .build()
                .user_data(EVENT_TOKEN);
            Some(entry)
        } else {
            None
        };

        // we has events, so we don't need to wait for timeout
        let nowait = event_e.is_some()
            || cq_is_not_empty
            || dur == Some(Duration::from_secs(0));

        let mut timeout_e = if let Some(dur) = dur.filter(|_| !nowait) {
            self.timeout.tv_sec = dur.as_secs() as _;
            self.timeout.tv_nsec = dur.subsec_nanos() as _;
            let entry = opcode::Timeout::new(&*self.timeout)
                .build()
                .user_data(TIMEOUT_TOKEN);
            Some(entry)
        } else {
            None
        };

        let n = event_e.is_some() as usize + timeout_e.is_some() as usize;
        if sq.capacity() - sq.len() < n {
            submitter.submit()?;
        }

        unsafe {
            if let Some(entry) = event_e.take() {
                let _ = sq.push(entry);
            }

            if let Some(entry) = timeout_e.take() {
                let _ = sq.push(entry);
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
        self.eventfd.clean();

        Ok(())
    }
}


#[derive(Clone)]
pub struct Handle {
    ring: Weak<RefCell<IoUring>>,
}

impl Handle {
    unsafe fn raw_push(&self, mut entry: squeue::Entry) -> io::Result<()> {
        let ring = self.ring.upgrade()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "Proactor closed"))?;

        let mut ring = ring.borrow_mut();
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

    pub unsafe fn push(&self, entry: squeue::Entry) -> io::Result<TicketFuture> {
        let (ticket, fut) = Ticket::new();
        let ptr = ticket.into_raw().as_ptr();

        self.raw_push(entry.user_data(ptr as _))?;

        Ok(fut)
    }
}
