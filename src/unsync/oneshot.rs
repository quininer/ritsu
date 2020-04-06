use std::pin::Pin;
use std::cell::RefCell;
use std::rc::{ Rc, Weak };
use std::task::{ Context, Waker, Poll };
use std::future::Future;
use futures_util::future::FusedFuture;
use io_uring::cqueue;
use crate::Ticket;


pub struct Sender<T>(Weak<RefCell<Inner<T>>>);

pub struct Receiver<T> {
    inner: Rc<RefCell<Inner<T>>>,
    is_end: bool
}

struct Inner<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Rc::new(RefCell::new(Inner {
        value: None,
        waker: None
    }));
    let inner2 = Rc::downgrade(&inner);

    (Sender(inner2), Receiver { inner, is_end: false })
}

impl<T> Sender<T> {
    pub fn send(self, item: T) -> Result<(), T> {
        if let Some(inner) = self.0.upgrade() {
            let mut inner = inner.borrow_mut();

            inner.value = Some(item);

            if let Some(waker) = inner.waker.take() {
                waker.wake()
            }

            Ok(())
        } else {
            Err(item)
        }
    }
}

impl Ticket for Sender<cqueue::Entry> {
    #[inline]
    fn into_raw(self) -> *const () {
        self.0.into_raw() as _
    }

    #[inline]
    unsafe fn from_raw(ptr: *const ()) -> Self {
        Sender(Weak::from_raw(ptr as *const RefCell<Inner<_>>))
    }

    #[inline]
    fn set(self, item: cqueue::Entry) {
        let _ = self.send(item);
    }
}

impl<T> Future for Receiver<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut inner = this.inner.borrow_mut();

        if let Some(val) = inner.value.take() {
            this.is_end = true;
            Poll::Ready(val)
        } else {
            inner.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl<T> FusedFuture for Receiver<T> {
    fn is_terminated(&self) -> bool {
        self.is_end
    }
}
