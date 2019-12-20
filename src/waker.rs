use std::{ io, mem };
use std::sync::{ Arc, Weak };
use std::os::unix::io::{ AsRawFd, RawFd };
use std::task::{ RawWaker, RawWakerVTable };


pub struct EventFd(Arc<Inner>);

#[derive(Clone)]
pub struct Waker(Weak<Inner>);

struct Inner(RawFd);

impl EventFd {
    pub fn new() -> io::Result<EventFd> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };

        if fd != -1 {
            let fd = Arc::new(Inner(fd));
            Ok(EventFd(fd))
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn waker(&self) -> Waker {
        let inner = Arc::downgrade(&self.0);
        Waker(inner)
    }
}

impl Waker {
    fn into_raw(self) -> *const () {
        self.0.into_raw() as _
    }

    unsafe fn from_raw(ptr: *const ()) -> Self {
        Waker(Weak::from_raw(ptr as *const Inner))
    }

    pub fn into_raw_waker(self) -> RawWaker {
        RawWaker::new(
            self.into_raw(),
            &RawWakerVTable::new(clone, wake, wake_by_ref, drop)
        )
    }

    pub fn wake(&self) {
        if let Some(fd) = self.0.upgrade() {
            const BUF: [u8; 8] = [1, 0, 0, 0, 0, 0, 0, 0];

            unsafe {
                libc::write(fd.0, BUF.as_ptr() as *const _, BUF.len() as _);

                // TODO fail
            }
        }
    }
}

impl AsRawFd for EventFd {
    fn as_raw_fd(&self) -> RawFd {
        (self.0).0
    }
}

unsafe fn clone(ptr: *const ()) -> RawWaker {
    let waker = Waker::from_raw(ptr);
    let waker2 = waker.clone();
    mem::forget(waker);
    waker2.into_raw_waker()
}

unsafe fn wake(ptr: *const ()) {
    Waker::from_raw(ptr).wake();
}

unsafe fn wake_by_ref(ptr: *const ()) {
    let waker = Waker::from_raw(ptr);
    waker.wake();
    mem::forget(waker);
}

unsafe fn drop(ptr: *const ()) {
    Waker::from_raw(ptr);
}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);

            // TODO log error
        }
    }
}
