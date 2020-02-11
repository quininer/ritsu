use std::fs::File;
use std::sync::{ atomic, Arc };
use std::io::{ self, Write };
use std::os::unix::io::{ FromRawFd, AsRawFd, RawFd };
use futures_task::ArcWake;


#[derive(Debug)]
pub struct EventFd {
    flag: atomic::AtomicBool,
    fd: File
}

impl EventFd {
    pub fn new() -> io::Result<EventFd> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };

        if fd != -1 {
            Ok(EventFd {
                flag: atomic::AtomicBool::new(true),
                fd: unsafe { File::from_raw_fd(fd) }
            })
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn get(&self) -> bool {
        self.flag.load(atomic::Ordering::Acquire)
    }

    pub fn clean(&self) {
        self.flag.store(false, atomic::Ordering::Release);
    }
}

impl ArcWake for EventFd {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        if !arc_self.flag.swap(true, atomic::Ordering::Acquire) {
            let _ = (&arc_self.fd).write(&0x1u64.to_le_bytes());
        }
    }
}

impl AsRawFd for EventFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
