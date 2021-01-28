//! fork from https://github.com/quininer/oneshot

#![allow(dead_code)]

use std::{ mem, ptr };
use std::pin::Pin;
use std::future::Future;
use std::task::{ Context, Waker, Poll };
use crate::loom::sync::atomic::{ AtomicU8, Ordering };
use crate::loom::cell::UnsafeCell;


pub struct Sender<T>(InlineRc<T>);
pub struct Receiver<T>(InlineRc<T>);

unsafe impl<T: Send> Send for InlineRc<T> {}
unsafe impl<T: Send> Sync for InlineRc<T> {}

struct InlineRc<T>(ptr::NonNull<Inner<T>>);

struct Inner<T> {
    state: AtomicU8,
    waker: UnsafeCell<mem::MaybeUninit<Waker>>,
    value: UnsafeCell<mem::MaybeUninit<T>>,
}

const WAKER_READY: u8 = 0b001;
const VALUE_READY: u8 = 0b010;
const CLOSED:      u8 = 0b100;


pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Box::new(Inner {
        waker: UnsafeCell::new(mem::MaybeUninit::uninit()),
        value: UnsafeCell::new(mem::MaybeUninit::uninit()),
        state: AtomicU8::new(0)
    });

    let raw_ptr = ptr::NonNull::from(Box::leak(inner));

    (Sender(InlineRc(raw_ptr)), Receiver(InlineRc(raw_ptr)))
}

impl<T> InlineRc<T> {
    #[inline]
    unsafe fn as_ref(&self) -> &Inner<T> {
        self.0.as_ref()
    }
}

impl<T> Sender<T> {
    /// Consumes the `Sender`, returning the raw pointer.
    #[inline]
    pub fn into_raw(self) -> ptr::NonNull<Sender<T>> {
        let ptr = (self.0).0.cast();
        mem::forget(self);
        ptr
    }

    /// # Safety
    ///
    /// Constructs an `Sender<T>` from a raw pointer.
    #[inline]
    pub unsafe fn from_raw(ptr: ptr::NonNull<Sender<T>>) -> Sender<T> {
        Sender(InlineRc(ptr.cast()))
    }

    pub fn send(self, entry: T) -> Result<(), T> {
        let this = unsafe { self.0.as_ref() };

        unsafe {
            this.value.with_mut(|ptr| (&mut *ptr).as_mut_ptr())
                .write(entry);
        }

        let state = this.state.fetch_or(VALUE_READY, Ordering::AcqRel);

        // The receiver is closed, We take value and return error.
        //
        // This will never fail because sender (self) is not closed.
        if state & CLOSED == CLOSED {
            this.state.fetch_and(!VALUE_READY, Ordering::AcqRel);

            let value = unsafe { take(&this.value) };

            return Err(value);
        }

        // take waker and wake it
        if state & WAKER_READY == WAKER_READY {
            let state = this.state.fetch_and(!WAKER_READY, Ordering::AcqRel);

            if state & WAKER_READY == WAKER_READY {
                unsafe {
                    take(&this.waker).wake();
                }
            }
        }

        Ok(())
    }

    #[inline]
    pub fn is_closed(&self) -> bool {
        let this = unsafe { self.0.as_ref() };

        this.state.load(Ordering::Relaxed) & CLOSED == CLOSED
    }
}

impl<T> Future for Receiver<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.0.as_ref() };

        // take waker
        let state = this.state.fetch_and(!WAKER_READY, Ordering::AcqRel);

        // check value and take it
        if state & VALUE_READY == VALUE_READY {
            this.state.fetch_and(!VALUE_READY, Ordering::AcqRel);

            let value = unsafe { take(&this.value) };

            return Poll::Ready(Some(value));
        }

        if state & CLOSED == CLOSED {
            return Poll::Ready(None);
        }

        // check waker
        if state & WAKER_READY == WAKER_READY {
            let waker_cx = cx.waker();
            let waker_ref = unsafe {
                let waker_ptr = this.waker
                    .with_mut(|ptr| (&mut *ptr).as_mut_ptr());
                &mut *waker_ptr
            };

            // replace waker if need
            if !waker_ref.will_wake(waker_cx) {
                let _ = mem::replace(waker_ref, waker_cx.clone());
            }
        } else {
            let waker_cx = cx.waker().clone();

            // never race with `send` because value has been checked.
            unsafe {
                this.waker.with_mut(|ptr| &mut *ptr)
                    .as_mut_ptr()
                    .write(waker_cx);
            }
        }

        // if channel is not closed, waker is always available after poll.
        let state = this.state.fetch_or(WAKER_READY, Ordering::AcqRel);

        // check value again
        if state & VALUE_READY == VALUE_READY {
            this.state.fetch_and(!VALUE_READY, Ordering::AcqRel);

            let value = unsafe { take(&this.value) };

            Poll::Ready(Some(value))
        } else {
            Poll::Pending
        }
    }
}

impl<T> Receiver<T> {
    #[inline]
    pub fn is_closed(&self) -> bool {
        let this = unsafe { self.0.as_ref() };

        this.state.load(Ordering::Relaxed) & CLOSED == CLOSED
    }
}

impl<T> Drop for InlineRc<T> {
    fn drop(&mut self) {
        let this = unsafe { self.0.as_ref() };

        let state = this.state.fetch_or(CLOSED, Ordering::AcqRel);

        // check reference count
        if state & CLOSED == CLOSED {
            unsafe {
                Box::from_raw(self.0.as_ptr());
            }
        }
    }
}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        // we can get state safely because we hold its ownership.
        let state = load_u8(&mut self.state);

        if state & WAKER_READY == WAKER_READY {
            unsafe { take(&self.waker) };
        }

        if state & VALUE_READY == VALUE_READY {
            unsafe { take(&self.value) };
        }
    }
}

#[inline]
unsafe fn take<T>(target: &UnsafeCell<mem::MaybeUninit<T>>) -> T {
    target.with_mut(|ptr| mem::replace(&mut *ptr, mem::MaybeUninit::uninit()))
        .assume_init()
}

#[cfg(feature = "loom")]
#[inline]
pub fn load_u8(t: &mut AtomicU8) -> u8 {
    unsafe {
        t.unsync_load()
    }
}

#[cfg(not(feature = "loom"))]
#[inline]
pub fn load_u8(t: &mut AtomicU8) -> u8 {
    *t.get_mut()
}
