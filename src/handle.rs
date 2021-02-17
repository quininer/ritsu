use io_uring::squeue;
use crate::{ LocalHandle, sq_submit };


pub trait Handle {
    unsafe fn push(&self, entry: squeue::Entry);
}

impl Handle for LocalHandle {
    unsafe fn push(&self, entry: squeue::Entry) {
        let mut ring = self.ring.borrow_mut();
        let (mut submitter, mut sq, mut cq) = ring.split();

        while sq.push(&entry).is_err() {
            sq_submit(&mut submitter, &mut sq, &mut cq, &self.eventfd).unwrap();
        }
    }
}

impl<T: Handle> Handle for &'_ T {
    unsafe fn push(&self, entry: squeue::Entry) {
        (**self).push(entry)
    }
}
