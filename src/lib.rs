mod loom;
mod ticket;
mod waker;

use std::{ io, mem };
use std::rc::Rc;
use std::pin::Pin;
use std::sync::Arc;
use std::ptr::NonNull;
use std::cell::RefCell;
use std::time::Duration;
use std::future::Future;
use std::task::{ Context, Poll };
use std::os::unix::io::AsRawFd;
use futures_task::{ self as task, WakerRef, Waker };
use io_uring::{
    types, opcode, squeue, cqueue,
    IoUring, Submitter
};
use waker::EventFd;
use ticket::Ticket;


#[derive(Clone)]
pub struct Proactor {
    ring: Rc<RefCell<IoUring>>,
    eventbuf: mem::ManuallyDrop<Box<[u8; 8]>>,
    eventfd: Arc<EventFd>,
}

pub struct Handle {
    ring: Rc<RefCell<IoUring>>
}

const WAKE_TOKEN: u64 = 0x0;

impl Proactor {
    pub fn new() -> io::Result<Proactor> {
        let ring = IoUring::new(256)?;
        let eventfd = EventFd::new()?;

        Ok(Proactor {
            ring: Rc::new(RefCell::new(ring)),
            eventbuf: mem::ManuallyDrop::new(Box::new([0; 8])),
            eventfd: Arc::new(eventfd)
        })
    }

    pub fn handle(&self) -> Handle {
        Handle {
            ring: Rc::clone(&self.ring)
        }
    }

    fn park(&mut self, dur: Option<Duration>) -> io::Result<()> {
        let mut ring = self.ring.borrow_mut();
        let (mut submitter, sq, cq) = ring.split();

        let (mut sq, mut cq) = (sq.available(), cq.available());
        let cq_is_not_empty = cq.len() != 0;

        // clean cq
        cq_consume(&mut cq);

        let state = self.eventfd.park();

        // we has events, so we don't need to wait for timeout
        let nowait = state.is_ready()
            || cq_is_not_empty
            || dur == Some(Duration::from_secs(0));

        if !state.is_parking() {
            let op = types::Fd(self.eventfd.as_raw_fd());
            let bufptr = self.eventbuf.as_mut_ptr();
            let entry = opcode::Read::new(op, bufptr, 8)
                .build()
                .user_data(WAKE_TOKEN);

            if sq.is_full() {
                sq_submit(&mut submitter, &mut sq, &mut cq)?;
            }

            unsafe {
                sq.push(entry).ok().unwrap();
            }
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
                cq_consume(&mut cq);
            } else {
                return Err(err);
            }
        }

        cq.sync();
        cq_consume(&mut cq);

        // reset eventfd
        self.eventfd.reset();

        Ok(())
    }

    pub fn block_on<F: Future>(&mut self, mut f: F) -> io::Result<F::Output> {
        {
            let mut ring = self.ring.borrow_mut();
            let (mut submitter, sq, cq) = ring.split();
            sq_submit(&mut submitter, &mut sq.available(), &mut cq.available())?;
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


fn cq_consume(cq: &mut cqueue::AvailableQueue) {
    for entry in cq {
        match entry.user_data() {
            WAKE_TOKEN => (),
            ptr => unsafe {
                Ticket::from_raw(NonNull::new_unchecked(ptr as _))
                    .send(entry);
            }
        }
    }
}

fn sq_submit(
    submitter: &mut Submitter,
    sq: &mut squeue::AvailableQueue,
    cq: &mut cqueue::AvailableQueue
) -> io::Result<()> {
    sq.sync();

    if sq.is_empty() {
        return Ok(())
    }

    while let Err(err) = submitter.submit() {
        if err.raw_os_error() == Some(libc::EBUSY) {
            cq.sync();
            cq_consume(cq);
        } else {
            return Err(err);
        }
    }

    Ok(())
}
