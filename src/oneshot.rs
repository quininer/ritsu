use std::rc::{ Rc, Weak };
use std::pin::Pin;
use std::cell::RefCell;
use std::task::{ Context, Waker, Poll };
use std::future::Future;
use futures::future::FusedFuture;
use crate::Ticket;
use crate::action::CompletionEntry;


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

impl Ticket for Sender<CompletionEntry> {
    fn into_raw(self) -> *const () {
        self.0.into_raw() as _
    }

    unsafe fn from_raw(ptr: *const ()) -> Self {
        Sender(Weak::from_raw(ptr as *const RefCell<Inner<_>>))
    }

    fn set(self, item: CompletionEntry) {
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

#[test]
fn test_it() {
    use std::thread;
    use std::sync::{ Arc, Mutex };
    use crate::Ticket;

    let (tx, rx) = channel::<()>();
    let tx = Arc::new(Mutex::new(tx));

    let h = thread::spawn(move || {
        let _ = tx.lock().unwrap().send(());
    });

    h.join().unwrap();
}
