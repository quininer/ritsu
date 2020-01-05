use std::fs::File;
use std::cell::Cell;
use std::sync::{ atomic, Arc };
use std::io::{ self, Write };
use std::os::unix::io::{ FromRawFd, AsRawFd, RawFd };
use futures_task::ArcWake;


#[derive(Debug)]
pub struct EventFd {
    flag: atomic::AtomicBool,
    fd: File
}

thread_local!{
    static ENTER: Cell<bool> = Cell::new(false);
}

impl EventFd {
    pub fn new() -> io::Result<EventFd> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };

        if fd != -1 {
            Ok(EventFd {
                flag: atomic::AtomicBool::new(false),
                fd: unsafe { File::from_raw_fd(fd) }
            })
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn take(&self) -> bool {
        self.flag.swap(false, atomic::Ordering::Relaxed)
    }
}

pub fn enter(f: impl FnOnce()) {
    ENTER.with(|flag| flag.set(true));
    f();
    ENTER.with(|flag| flag.set(false));
}

impl ArcWake for EventFd {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        if ENTER.with(|flag| flag.get()) {
            return;
        }

        if !arc_self.flag.swap(true, atomic::Ordering::Relaxed) {
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
