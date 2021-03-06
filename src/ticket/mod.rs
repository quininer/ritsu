pub mod oneshot;

use std::ptr;
use std::pin::Pin;
use std::task::{ Context, Poll };
use std::future::Future;
use pin_project_lite::pin_project;
use io_uring::cqueue;


pub struct Ticket(oneshot::Sender<cqueue::Entry>);

impl Ticket {
    #[inline]
    pub fn new() -> (Ticket, TicketFuture) {
        let (tx, rx) = oneshot::channel();

        (Ticket(tx), TicketFuture { fut: rx })
    }

    #[inline]
    pub(crate) fn into_raw(self) -> ptr::NonNull<Ticket> {
        self.0.into_raw().cast()
    }

    #[inline]
    pub(crate) unsafe fn from_raw(ptr: ptr::NonNull<Ticket>) -> Ticket {
        Ticket(oneshot::Sender::from_raw(ptr.cast()))
    }

    #[inline]
    pub(crate) fn send(self, entry: cqueue::Entry) {
        let _ = self.0.send(entry);
    }
}

pin_project!{
    pub struct TicketFuture {
        #[pin]
        fut: oneshot::Receiver<cqueue::Entry>
    }
}

impl Future for TicketFuture {
    type Output = cqueue::Entry;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.fut.poll(cx) {
            Poll::Ready(Some(entry)) => Poll::Ready(entry),
            Poll::Ready(None) | Poll::Pending => Poll::Pending
        }
    }
}

impl TicketFuture {
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.fut.is_closed()
    }

    #[inline]
    pub fn as_ptr(&self) -> ptr::NonNull<TicketFuture> {
        self.fut.as_ptr().cast()
    }
}
