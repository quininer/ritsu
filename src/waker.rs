use std::fs::File;
use std::sync::{ atomic, Arc };
use std::io::{ self, Write };
use std::os::unix::io::{ FromRawFd, AsRawFd, RawFd };
use futures_task::ArcWake;


#[derive(Debug)]
pub struct EventFd {
    flag: atomic::AtomicU8,
    fd: File
}

#[derive(Copy, Clone)]
pub struct State(u8);

const READY:   u8 = 0b01;
const PARKING: u8 = 0b10;

impl EventFd {
    pub fn new() -> io::Result<EventFd> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };

        if fd != -1 {
            Ok(EventFd {
                flag: atomic::AtomicU8::new(0x00),
                fd: unsafe { File::from_raw_fd(fd) }
            })
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[inline]
    pub fn park(&self) -> State {
        let state = self.flag.fetch_or(PARKING, atomic::Ordering::AcqRel);
        State(state)
    }

    #[inline]
    pub fn reset(&self) {
        self.flag.fetch_and(!READY, atomic::Ordering::Release);
    }
}

impl State {
    #[inline]
    pub fn is_ready(self) -> bool {
        self.0 & READY == READY
    }

    #[inline]
    pub fn is_park(self) -> bool {
        self.0 & PARKING == PARKING
    }
}

impl ArcWake for EventFd {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        let EventFd { flag, fd } = &**arc_self;

        let state = State(flag.fetch_and(!READY, atomic::Ordering::AcqRel));

        if !state.is_ready() && state.is_park() {
            let _ = (fd as &File).write(&0x1u64.to_le_bytes());

            flag.fetch_and(!PARKING, atomic::Ordering::Release);
        }
    }
}

impl AsRawFd for EventFd {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
