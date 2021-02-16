use io_uring::squeue;
use crate::{ LocalHandle, sq_submit };


pub trait Handle {
    unsafe fn push(&self, entry: squeue::Entry);
}

impl Handle for LocalHandle {
    unsafe fn push(&self, mut entry: squeue::Entry) {
        let mut ring = self.ring.borrow_mut();
        let (mut submitter, sq, cq) = ring.split();
        let mut sq = sq.available();

        while let Err(entry2) = sq.push(entry) {
            entry = entry2;
            sq_submit(&mut submitter, &mut sq, &mut cq.available(), &self.eventfd).unwrap();
        }
    }
}
