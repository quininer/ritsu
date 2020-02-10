pub(crate) mod oneshot;

use std::io;
use std::cell::RefCell;
use std::rc::{ Rc, Weak };
use io_uring::IoUring;
use crate::{
    Proactor,
    Handle as TaskHandle,
    Ticket,
    SubmissionEntry, CompletionEntry,
    cq_drain
};


#[derive(Clone)]
pub struct Handle {
    ring: Weak<RefCell<IoUring>>
}

impl Proactor<Handle> {
    pub fn handle(&self) -> Handle {
        Handle { ring: Rc::downgrade(&self.ring) }
    }
}

impl TaskHandle for Handle {
    type Ticket = oneshot::Sender<CompletionEntry>;
    type Wait = oneshot::Receiver<CompletionEntry>;

    unsafe fn push(&self, entry: SubmissionEntry) -> io::Result<Self::Wait> {
        let ring = self.ring.upgrade()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "Proactor closed"))?;

        let (tx, rx) = oneshot::channel();

        let mut ring = ring.borrow_mut();
        let (submitter, sq, cq) = ring.split();
        let mut entry = entry.user_data(tx.into_raw() as _);

        loop {
            let mut sq = sq.available();

            match sq.push(entry) {
                Ok(_) => break,
                Err(e) => entry = e
            }

            match submitter.submit() {
                Ok(_) => (),
                Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => {
                    cq_drain::<Self::Ticket>(&mut cq.available());
                    submitter.submit()?;
                },
                Err(err) => return Err(err)
            }
        }

        Ok(rx)
    }
}
