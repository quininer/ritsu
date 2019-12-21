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
use crate::action::{ SubmissionEntry, CompletionEntry, Action };


const ZERO_DURATION: Duration = Duration::from_secs(0);
const WAKEUP_TOKEN: usize = 0x00;
const EVENT_TOKEN: usize = 0x01;
const EVENT_EMPTY: [u8; 8] = [0; 8];

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
        todo!()
    }

    pub fn park(&mut self, dur: Option<Duration>) -> io::Result<()> {
        let mut ring = self.ring.borrow_mut();

        if &EVENT_EMPTY != &*self.eventbuf {
            unsafe {
                let mut bufs = [io::IoSliceMut::new(&mut self.eventbuf[..])];

                let entry = opcode::Readv::new(
                    opcode::Target::Fd(self.eventfd.as_raw_fd()),
                    bufs.as_mut_ptr() as *mut _,
                    1
                )
                    .build()
                    .user_data(EVENT_TOKEN as _);
            }
        }

        if let Some(dur) = dur {
            if dur != ZERO_DURATION {
                unsafe {
                    self.timeout.tv_sec = dur.as_secs() as _;
                    self.timeout.tv_nsec = dur.subsec_nanos() as _;

                    let entry = opcode::Timeout::new(&*self.timeout)
                        .build()
                        .user_data(WAKEUP_TOKEN as _);
                }
            }
        }

        // TODO

        ring.submit_and_wait(1)?;

        for entry in ring.completion().available() {
            unsafe {
                let sender = oneshot::Sender::from_raw(entry.user_data() as _);
                let _ = sender.send(entry);
            }
        }

        Ok(())
    }
}
