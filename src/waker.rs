use std::{ io, mem };
use std::sync::{ Arc, Weak };
use std::os::unix::io::RawFd;
use std::task::{ RawWaker, RawWakerVTable };


#[derive(Clone)]
pub struct Waker(Weak<RawFd>);

pub fn create() -> io::Result<(Arc<RawFd>, Waker)> {
    let fd = unsafe {
        libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK)
    };

    if fd != -1 {
        let fd = Arc::new(fd);
        let waker = Waker(Arc::downgrade(&fd));
        Ok((fd, waker))
    } else {
        Err(io::Error::last_os_error())
    }
}

impl Waker {
    fn into_raw(self) -> *const () {
        self.0.into_raw() as _
    }

    unsafe fn from_raw(ptr: *const ()) -> Self {
        Waker(Weak::from_raw(ptr as *const RawFd))
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
                libc::write(*fd, BUF.as_ptr() as *const _, BUF.len() as _);

                // TODO fail
            }
        }
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
