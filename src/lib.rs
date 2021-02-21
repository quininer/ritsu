mod loom;
mod ticket;
mod waker;
mod handle;
pub mod actions;

use std::io;
use std::rc::Rc;
use std::pin::Pin;
use std::sync::Arc;
use std::ptr::NonNull;
use std::cell::RefCell;
use std::time::Duration;
use std::future::Future;
use std::task::{ Context, Poll };
use std::os::unix::io::AsRawFd;
use futures_task as task;
use io_uring::{
    types, opcode,
    IoUring, Submitter,
    SubmissionQueue, CompletionQueue
};
pub use ticket::{ Ticket, TicketFuture };
pub use handle::Handle;
pub use waker::EventFd;


pub struct Proactor {
    ring: Rc<RefCell<IoUring>>,
    eventbuf: Box<[u8; 8]>,
    eventfd: Arc<EventFd>,
}

#[derive(Clone)]
pub struct LocalHandle {
    ring: Rc<RefCell<IoUring>>,
    eventfd: Arc<EventFd>,
}

const WAKE_TOKEN: u64 = 0x0;
const EMPTY_TOKEN: u64 = 0x1;

impl Proactor {
    pub fn new() -> io::Result<Proactor> {
        let ring = IoUring::new(256)?;
        let eventfd = EventFd::new()?;

        Ok(Proactor {
            ring: Rc::new(RefCell::new(ring)),
            eventbuf: Box::new([0; 8]),
            eventfd: Arc::new(eventfd)
        })
    }

    pub fn handle(&self) -> LocalHandle {
        LocalHandle {
            ring: Rc::clone(&self.ring),
            eventfd: Arc::clone(&self.eventfd)
        }
    }

    pub fn waker(&self) -> &Arc<EventFd> {
        &self.eventfd
    }

    pub fn park(&mut self, dur: Option<Duration>) -> io::Result<()> {
        let mut ring = self.ring.borrow_mut();
        let (mut submitter, mut sq, mut cq) = ring.split();

        // clean cq
        cq_consume(&mut cq, &self.eventfd);

        let state = self.eventfd.park();

        // we has events, so we don't need to wait for timeout
        let nowait = state.is_ready()
            || dur == Some(Duration::from_secs(0));

        if !state.is_parking() {
            let op = types::Fd(self.eventfd.as_raw_fd());
            let bufptr = self.eventbuf.as_mut_ptr();
            let entry = opcode::Read::new(op, bufptr, 8)
                .build()
                .user_data(WAKE_TOKEN);

            if sq.is_full() {
                sq_submit(&mut submitter, &mut sq, &mut cq, &self.eventfd)?;
            }

            unsafe {
                sq.push(&entry).unwrap();
            }

            drop(sq);
        };

        while let Err(err) =
            if nowait {
                submitter.submit()
            } else if let Some(dur) = dur {
                let timespec = types::Timespec::new()
                    .sec(dur.as_secs())
                    .nsec(dur.subsec_nanos());
                let args = types::SubmitArgs::new()
                    .timespec(&timespec);
                submitter.submit_with_args(1, &args)
            } else {
                submitter.submit_and_wait(1)
            }
        {
            if err.raw_os_error() == Some(libc::EBUSY) {
                cq.sync();
                cq_consume(&mut cq, &self.eventfd);
            } else {
                return Err(err);
            }
        }

        cq.sync();
        cq_consume(&mut cq, &self.eventfd);

        // reset eventfd
        self.eventfd.reset();

        Ok(())
    }

    pub fn block_on<F: Future>(&mut self, mut f: F) -> io::Result<F::Output> {
        {
            let mut ring = self.ring.borrow_mut();
            let (mut submitter, mut sq, mut cq) = ring.split();
            sq_submit(&mut submitter, &mut sq, &mut cq, &self.eventfd)?;
        }

        loop {
            let waker = task::waker_ref(&self.eventfd);
            let mut cx = Context::from_waker(&waker);

            let f = unsafe {
                Pin::new_unchecked(&mut f)
            };

            if let Poll::Ready(val) = f.poll(&mut cx) {
                return Ok(val);
            }

            self.park(None)?;
        }
    }
}


fn cq_consume(cq: &mut CompletionQueue<'_>, eventfd: &EventFd) {
    for entry in cq {
        match entry.user_data() {
            WAKE_TOKEN => eventfd.unpark(),
            EMPTY_TOKEN => (),
            ptr => unsafe {
                Ticket::from_raw(NonNull::new_unchecked(ptr as _))
                    .send(entry);
            }
        }
    }
}

fn sq_submit(
    submitter: &mut Submitter,
    sq: &mut SubmissionQueue<'_>,
    cq: &mut CompletionQueue<'_>,
    eventfd: &EventFd
) -> io::Result<()> {
    sq.sync();

    if sq.is_empty() {
        return Ok(())
    }

    let mut count = 0;

    while let Err(err) = submitter.submit() {
        if err.raw_os_error() == Some(libc::EBUSY) && count < 3 {
            count += 1;
            cq.sync();
            cq_consume(cq, eventfd);
        } else {
            return Err(err);
        }
    }

    Ok(())
}

impl Drop for Proactor {
    fn drop(&mut self) {
        if self.eventfd.load().is_parking() {
            let mut ring = self.ring.borrow_mut();
            proactor_drop(&mut ring, &self.eventfd).unwrap();
        }
    }
}

#[cold]
fn proactor_drop(ring: &mut IoUring, eventfd: &EventFd) -> io::Result<()> {
    let (mut submitter, mut sq, mut cq) = ring.split();

    for entry in &mut cq {
        if entry.user_data() == WAKE_TOKEN {
            return Ok(());
        }
    }

    let cancel_e = opcode::AsyncCancel::new(WAKE_TOKEN)
        .build()
        .user_data(EMPTY_TOKEN);

    unsafe {
        while sq.push(&cancel_e).is_err() {
            sq_submit(&mut submitter, &mut sq, &mut cq, eventfd)?;
        }
    }

    sq.sync();

    loop {
        match submitter.submit_and_wait(1) {
            Ok(_) => (),
            Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => (),
            Err(err) => return Err(err)
        }

        cq.sync();

        for entry in &mut cq {
            if entry.user_data() == WAKE_TOKEN {
                return Ok(());
            }
        }
    }
}
