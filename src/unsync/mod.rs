pub(crate) mod oneshot;

use std::{ io, mem };
use std::pin::Pin;
use std::task::{ Context, Poll };
use std::future::Future;
use io_uring::{ cqueue, squeue };
use crate::{
    Proactor, RawHandle,
    Handle as TaskHandle,
    Ticket,
};


#[derive(Clone)]
pub struct Handle {
    handle: RawHandle<Self>,
}

impl Proactor<Handle> {
    pub fn handle(&self) -> Handle {
        Handle { handle: self.as_raw_handle() }
    }
}

impl TaskHandle for Handle {
    type Ticket = oneshot::Sender<cqueue::Entry>;
    type Wait = AndFuture<oneshot::Receiver<cqueue::Entry>, io::Error>;

    unsafe fn push(&self, entry: squeue::Entry) -> Self::Wait {
        let (tx, rx) = oneshot::channel();

        match self.handle.push(entry.user_data(tx.into_raw() as _)) {
            Ok(()) => AndFuture(Inner::Fut(rx)),
            Err(err) => AndFuture(Inner::Err(err))
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
