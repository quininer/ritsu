use std::{ io as std_io, mem };
use std::pin::Pin;
use std::task::{ Context, Poll };
use std::future::Future;
use tokio::runtime;
use tokio::task::JoinHandle;
use tokio::sync::mpsc;
use pin_project_lite::pin_project;
use ritsu::action::{ Handle as TaskHandle, HandleVTable };
use ritsu::{
    RawHandle,
    Ticket, TicketFuture,
    SubmissionEntry
};


#[derive(Clone)]
pub struct Handle {
    inner: InnerHandle,
    tokio: runtime::Handle
}

#[derive(Clone)]
struct InnerHandle(mpsc::UnboundedSender<SubmissionEntry>);

pub struct Driver(mpsc::UnboundedReceiver<SubmissionEntry>);

impl Handle {
    pub fn new(tokio: runtime::Handle) -> (Driver, Handle) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Driver(rx), Handle { inner: InnerHandle(tx), tokio })
    }

    pub fn enter<R, F: FnOnce() -> R>(&self, f: F) -> R {
        let handle = create_handle(self.inner.clone());

        unsafe {
            ritsu::handle::set(handle);
        }

        f()
    }

    pub fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static
    {

        self.tokio.spawn(WithFuture {
            handle: self.inner.clone(),
            fut
        })
    }
}

impl Driver {
    pub async fn register(mut self, handle: RawHandle) -> std_io::Result<()> {
        while let Some(sqe) = self.0.recv().await {
            unsafe {
                handle.raw_push(sqe)?;
            }
        }

        Ok(())
    }
}

fn create_handle(handle: InnerHandle) -> TaskHandle {
    static VTABLE: HandleVTable = HandleVTable {
        push, clone, drop
    };

    unsafe fn push(ptr: *const (), entry: SubmissionEntry) -> std_io::Result<TicketFuture> {
        let handle = Box::from_raw(ptr as *mut InnerHandle);

        let (ticket, fut) = Ticket::new();

        let ret = handle.0.send(ticket.register(entry));
        mem::forget(handle);

        ret.map_err(|_| std_io::Error::new(
            std_io::ErrorKind::Other,
            "tokio-ritsu driver closed"
        ))?;

        Ok(fut)
    }

    unsafe fn clone(ptr: *const ()) -> TaskHandle {
        let handle = Box::from_raw(ptr as *mut InnerHandle);
        let handle2 = InnerHandle::clone(&handle);
        mem::forget(handle);

        create_handle(handle2)
    }

    unsafe fn drop(ptr: *const ()) {
        Box::from_raw(ptr as *mut InnerHandle);
    }

    let handle = Box::new(handle);

    unsafe {
        TaskHandle::new(Box::into_raw(handle) as *const (), &VTABLE)
    }
}


pin_project!{
    struct WithFuture<F> {
        handle: InnerHandle,
        #[pin]
        fut: F
    }
}

impl<F: Future> Future for WithFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let handle = create_handle(this.handle.clone());

        unsafe {
            ritsu::handle::set(handle);
        }

        this.fut.poll(cx)
    }
}
