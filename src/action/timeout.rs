use std::{ io, mem };
use std::time::Duration;
use io_uring::opcode::{ self, types };
use crate::Handle;


pub struct Timer {
    timespec: mem::ManuallyDrop<Box<types::Timespec>>,
    lock: bool,
    handle: Handle
}

impl Timer {
    pub fn new(handle: Handle) -> Timer {
        Timer {
            timespec: mem::ManuallyDrop::new(Box::new(types::Timespec::default())),
            lock: false,
            handle
        }
    }

    pub async fn delay_for(&mut self, dur: Duration) -> io::Result<()> {
        debug_assert!(!self.lock);

        self.timespec.tv_sec = dur.as_secs() as _;
        self.timespec.tv_nsec = dur.subsec_nanos() as _;

        let entry = opcode::Timeout::new(&**self.timespec).build();
        self.lock = true;
        let ret = safety_await!{
            [];
            unsafe { self.handle.push(entry) }
        };
        self.lock = false;
        let ret = ret?.result();

        if ret >= 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    // TODO delay_until
}

impl Drop for Timer {
    fn drop(&mut self) {
        if !self.lock {
            unsafe {
                mem::ManuallyDrop::drop(&mut self.timespec);
            }
        }
    }
}
