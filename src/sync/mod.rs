pub mod oneshot;

use std::ptr;
use std::pin::Pin;
use std::task::{ Context, Poll };
use std::future::Future;
use pin_project_lite::pin_project;
use crate::{ SubmissionEntry, CompletionEntry };


pub struct Ticket(oneshot::Sender<CompletionEntry>);

impl Ticket {
    #[inline]
    pub fn new() -> (Ticket, TicketFuture) {
        let (tx, rx) = oneshot::channel();

        (Ticket(tx), TicketFuture { fut: rx })
    }

    #[inline]
    pub fn register(self, entry: SubmissionEntry) -> SubmissionEntry {
        entry.user_data(self.0.into_raw() as _)
    }

    #[inline]
    pub(crate) unsafe fn from_raw(ptr: ptr::NonNull<Ticket>) -> Ticket {
        Ticket(oneshot::Sender::from_raw(ptr.cast()))
    }

    #[inline]
    pub(crate) fn send(self, entry: CompletionEntry) {
        let _ = self.0.send(entry);
    }
}

pin_project!{
    pub struct TicketFuture {
        #[pin]
        fut: oneshot::Receiver<CompletionEntry>
    }
}

impl Future for TicketFuture {
    type Output = CompletionEntry;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.fut.poll(cx) {
            Poll::Ready(Some(entry)) => Poll::Ready(entry),
            Poll::Ready(None) | Poll::Pending => Poll::Pending
        }
    }
}
