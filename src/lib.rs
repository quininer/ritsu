#![feature(weak_into_raw)]

mod channel;
mod oneshot;
mod waker;
mod executor;
pub mod action;

use std::io;
use std::rc::{ Rc, Weak };
use std::cell::RefCell;
use std::time::Duration;
use std::marker::PhantomData;
use std::os::unix::io::AsRawFd;
use io_uring::{ squeue, cqueue, opcode, IoUring };
use crate::waker::{ EventFd, Waker };
use crate::channel::{ Channel, Sender };
use crate::action::{ SubmissionEntry, CompletionEntry };


const EVENT_EMPTY: [u8; 8] = [0; 8];
const EVENT_TOKEN: u64 = 0x00;
const TIMEOUT_TOKEN: u64 = 0x00u64.wrapping_sub(1);

pub struct Proactor<C: Channel<CompletionEntry>> {
    ring: Rc<RefCell<IoUring>>,
    eventfd: EventFd,
    eventbuf: Box<[u8; 8]>,
    timeout: Box<opcode::Timespec>,
    _mark: PhantomData<C>
}

pub struct Handle<C: Channel<CompletionEntry>> {
    ring: Weak<RefCell<IoUring>>,
    _mark: PhantomData<C>
}

impl<C: Channel<CompletionEntry>> Handle<C> {
    pub fn push(&self, ticket: C::Sender, entry: SubmissionEntry) -> io::Result<()> {
        let ring = self.ring.upgrade()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "Proactor closed"))?;
        let mut ring = ring.borrow_mut();

        unsafe {
            let mut entry = entry.user_data(ticket.into_raw() as _);

            loop {
                match ring.submission().available().push(entry) {
                    Ok(_) => break,
                    Err(output) => entry = output
                }

                ring.submit()?;
            }
        }

        Ok(())
    }
}

impl<C: Channel<CompletionEntry>> Proactor<C> {
    pub fn unpark(&self) -> Waker {
        self.eventfd.waker()
    }

    fn maybe_event(&mut self) -> Option<squeue::Entry> {
        if &EVENT_EMPTY == &*self.eventbuf {
            return None;
        }

        self.eventbuf.copy_from_slice(&EVENT_EMPTY);

        let mut bufs = [io::IoSliceMut::new(&mut self.eventbuf[..])];
        let op = opcode::Target::Fd(self.eventfd.as_raw_fd());
        let bufs_ptr = bufs.as_mut_ptr() as *mut _;

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
                    let sender = oneshot::Sender::from_raw(ptr as _);
                    let _ = sender.send(entry);
                }
            }
        }

        Ok(())
    }
}
