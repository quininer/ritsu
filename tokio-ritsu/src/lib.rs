mod oneshot;

use std::{ io, mem };
use std::pin::Pin;
use std::task::{ Context, Poll };
use std::future::Future;
use tokio::sync::mpsc;
use ritsu::{
    Handle as TaskHandle, Ticket,
    RawHandle, SubmissionEntry
};


#[derive(Clone)]
pub struct Handle(mpsc::UnboundedSender<SubmissionEntry>);

pub struct Driver(mpsc::UnboundedReceiver<SubmissionEntry>);

impl Handle {
    pub fn new() -> (Driver, Handle) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Driver(rx), Handle(tx))
    }
}

impl Driver {
    pub async fn register(mut self, handle: RawHandle<Handle>) -> io::Result<()> {
        while let Some(sqe) = self.0.recv().await {
            unsafe {
                handle.push(sqe)?;
            }
        }

        Ok(())
    }
}

impl TaskHandle for Handle {
    type Ticket = oneshot::Sender;
    type Wait = AndFuture<oneshot::Receiver, io::Error>;

    unsafe fn push(&self, entry: SubmissionEntry) -> Self::Wait {
        let (tx, rx) = oneshot::channel();

        match self.0.send(entry.user_data(tx.into_raw() as _)) {
            Ok(()) => AndFuture(Inner::Fut(rx)),
            Err(_) => AndFuture(Inner::Err(io::Error::new(
                io::ErrorKind::Other,
                "tokio-ritsu driver closed"
            )))
        }
    }
}

pub struct AndFuture<F, E>(Inner<F, E>);

enum Inner<F, E> {
    Fut(F),
    Err(E),
    End
}

impl<F, E> Future for AndFuture<F, E>
where
    F: Future + Unpin,
    E: Unpin
{
    type Output = Result<F::Output, E>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match mem::replace(&mut self.0, Inner::End) {
            Inner::Fut(mut f) => match Pin::new(&mut f).poll(cx) {
                Poll::Ready(output) => Poll::Ready(Ok(output)),
                Poll::Pending => {
                    self.0 = Inner::Fut(f);
                    Poll::Pending
                }
            },
            Inner::Err(err) => Poll::Ready(Err(err)),
            Inner::End => panic!()
        }
    }
}
