use std::mem;
use std::fs::File;
use std::io::{ self, Write };
use std::sync::{ Arc, Weak };
use std::os::unix::io::{ FromRawFd, AsRawFd, RawFd };
use std::task::{ RawWaker, RawWakerVTable };


pub struct EventFd(Arc<File>);

#[derive(Clone)]
pub struct Waker(Weak<File>);

impl EventFd {
    pub fn new() -> io::Result<EventFd> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };

        if fd != -1 {
            let fd = Arc::new(unsafe { File::from_raw_fd(fd) });
            Ok(EventFd(fd))
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn waker(&self) -> Waker {
        Waker(Arc::downgrade(&self.0))
    }
}

impl Waker {
    fn into_raw(self) -> *const () {
        self.0.into_raw() as _
    }

    unsafe fn from_raw(ptr: *const ()) -> Self {
        Waker(Weak::from_raw(ptr as *const File))
    }

    pub fn into_raw_waker(self) -> RawWaker {
        RawWaker::new(
            self.into_raw(),
            &RawWakerVTable::new(clone, wake, wake_by_ref, drop)
        )
    }

    pub fn wake(&self) {
        if let Some(fd) = self.0.upgrade() {
            let _ = (&*fd).write(&0x1u64.to_le_bytes());

            // TODO log fail
        }
    }
}

impl AsRawFd for EventFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
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
