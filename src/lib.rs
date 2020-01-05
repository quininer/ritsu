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
use io_uring::opcode::{ self, types };
use io_uring::{ squeue, cqueue, IoUring };
use crate::waker::{ EventFd, enter };


pub type SubmissionEntry = squeue::Entry;
pub type CompletionEntry = cqueue::Entry;
pub type LocalHandle = Handle<oneshot::Sender<CompletionEntry>>;

const EVENT_TOKEN: u64 = 0x00;
const TIMEOUT_TOKEN: u64 = 0x00u64.wrapping_sub(1);

pub struct Proactor<C: Ticket> {
    ring: Rc<RefCell<IoUring>>,
    eventfd: Arc<EventFd>,

    #[allow(dead_code)]
    event_buf: Box<[u8; 8]>,

    event_iovec: Box<[libc::iovec; 1]>,
    timeout: Box<types::Timespec>,
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

fn cq_drain<C: Ticket>(cq: &mut cqueue::AvailableQueue) {
    for entry in cq {
        match entry.user_data() {
            EVENT_TOKEN | TIMEOUT_TOKEN => (),
            ptr => unsafe {
                C::from_raw(ptr as _).set(entry.clone());
            }
        }
    }
}

impl<C: Ticket> Proactor<C> {
    pub fn new() -> io::Result<Proactor<C>> {
        let ring = io_uring::IoUring::new(256)?; // TODO better number
        let mut event_buf = Box::new([0; 8]);
        let event_bufptr =
            unsafe { mem::transmute::<_, libc::iovec>(io::IoSliceMut::new(&mut *event_buf)) };
        let event_iovec = Box::new([event_bufptr]);

        Ok(Proactor {
            ring: Rc::new(RefCell::new(ring)),
            eventfd: Arc::new(EventFd::new()?),
            event_buf, event_iovec,
            timeout: Box::new(types::Timespec::default()),
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

    pub fn park(&mut self, dur: Option<Duration>) -> io::Result<()> {
        let mut ring = self.ring.borrow_mut();
        let (submitter, sq, cq) = ring.split();
        let (mut sq, mut cq) = (sq.available(), cq.available());
        let cq_is_not_empty = cq.len() != 0;

        // handle before eventfd to avoid unnecessary wakeup
        cq_drain::<C>(&mut cq);

        let mut event_e = if self.eventfd.take() {
            let op = types::Target::Fd(self.eventfd.as_raw_fd());
            let iovec_ptr = self.event_iovec.as_mut_ptr();
            let entry = opcode::Readv::new(op, iovec_ptr, 1)
                .build()
                .user_data(EVENT_TOKEN);
            Some(entry)
        } else {
            None
        };

        let nowait = event_e.is_some()
            || cq_is_not_empty
            || dur == Some(Duration::from_secs(0));

        // we has events, so we don't need to wait for timeout
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

        // avoid wake self
        enter(|| cq_drain::<C>(&mut cq));

        Ok(())
    }
}

impl<C: Ticket> Handle<C> {
    pub unsafe fn push(&self, sender: C, entry: SubmissionEntry) -> io::Result<()> {
        let ring = self.ring.upgrade()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "Proactor closed"))?;

        let mut ring = ring.borrow_mut();
        let (submitter, sq, cq) = ring.split();
        let mut entry = entry.user_data(sender.into_raw() as _);

        loop {
            let mut sq = sq.available();

            match sq.push(entry) {
                Ok(_) => break,
                Err(e) => entry = e
            }

            match submitter.submit() {
                Ok(_) => (),
                Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => {
                    cq_drain::<C>(&mut cq.available());
                    submitter.submit()?;
                },
                Err(err) => return Err(err)
            }
        }

        Ok(())
    }
}
