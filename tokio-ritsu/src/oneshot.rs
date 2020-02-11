use std::pin::Pin;
use std::future::Future;
use std::task::{ Context, Poll };
use tokio::sync::oneshot;
use ritsu::{ Ticket, CompletionEntry };


pub struct Sender(oneshot::Sender<CompletionEntry>);

pub struct Receiver(oneshot::Receiver<CompletionEntry>);

#[inline]
pub fn channel() -> (Sender, Receiver) {
    let (tx, rx) = oneshot::channel();
    (Sender(tx), Receiver(rx))
}

impl Ticket for Sender {
    #[inline]
    fn into_raw(self) -> *const () {
        self.0.into_raw() as _
    }

    #[inline]
    unsafe fn from_raw(ptr: *const ()) -> Self {
        Sender(oneshot::Sender::from_raw(ptr as *const CompletionEntry))
    }

    #[inline]
    fn set(self, item: CompletionEntry) {
        let _ = self.0.send(item);
    }
}

impl Future for Receiver {
    type Output = CompletionEntry;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.0).poll(cx) {
            Poll::Ready(Ok(cqe)) => Poll::Ready(cqe),
            Poll::Ready(Err(_)) | Poll::Pending => Poll::Pending
        }
    }
}
