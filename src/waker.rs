use std::fs::File;
use std::sync::{ atomic, Arc };
use std::io::{ self, Write };
use std::os::unix::io::{ FromRawFd, AsRawFd, RawFd };
use futures_task::ArcWake;


#[derive(Debug)]
pub struct EventFd {
    fd: File,
    flag: atomic::AtomicBool
}

impl EventFd {
    pub fn new() -> io::Result<EventFd> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };

        if fd != -1 {
            Ok(EventFd {
                fd: unsafe { File::from_raw_fd(fd) },
                flag: atomic::AtomicBool::new(false)
            })
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn clean(&self) {
        self.flag.store(false, atomic::Ordering::Relaxed);
    }
}

impl ArcWake for EventFd {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        if !arc_self.flag.fetch_and(true, atomic::Ordering::Relaxed) {
            let _ = (&arc_self.fd).write(&0x1u64.to_le_bytes());
        }

        // TODO log fail
    }
}

impl AsRawFd for EventFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
