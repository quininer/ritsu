use io_uring::squeue;
use crate::ticket::{ Ticket, TicketFuture };
use crate::{ LocalHandle, sq_submit };


pub trait Handle {
    unsafe fn push(&self, entry: squeue::Entry) -> TicketFuture;
}

impl Handle for LocalHandle {
    unsafe fn push(&self, entry: squeue::Entry) -> TicketFuture {
        let (tx, rx) = Ticket::new();
        let mut entry = tx.register(entry);

        let mut ring = self.ring.borrow_mut();
        let (mut submitter, sq, cq) = ring.split();
        let mut sq = sq.available();

        while let Err(entry2) = sq.push(entry) {
            entry = entry2;
            sq_submit(&mut submitter, &mut sq, &mut cq.available()).unwrap();
        }

        rx
    }
}
