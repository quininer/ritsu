use std::fs::File;
use std::sync::Arc;
use std::io::{ self, Write };
use std::os::unix::io::{ FromRawFd, AsRawFd, RawFd };
use futures_task::ArcWake;


#[derive(Debug)]
pub struct EventFd(File);

impl EventFd {
    pub fn new() -> io::Result<EventFd> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };

        if fd != -1 {
            Ok(EventFd(unsafe { File::from_raw_fd(fd) }))
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

impl ArcWake for EventFd {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        let _ = (&arc_self.0).write(&0x1u64.to_le_bytes());

        // TODO log fail
    }
}

impl AsRawFd for EventFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}
