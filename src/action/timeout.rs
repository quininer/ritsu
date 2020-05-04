use std::io;
use std::time::Duration;
use io_uring::opcode::{ self, types };
use crate::handle;
use crate::util::MaybeLock;


pub struct Timer {
    timespec: MaybeLock<Box<types::Timespec>>
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            timespec: MaybeLock::new(Box::new(types::Timespec::default()))
        }
    }

    pub async fn delay_for(&mut self, dur: Duration) -> io::Result<()> {
        self.timespec.tv_sec = dur.as_secs() as _;
        self.timespec.tv_nsec = dur.subsec_nanos() as _;

        let entry = opcode::Timeout::new(&**self.timespec).build();
        let ret = safety_await!{
            ( self.timespec );
            unsafe { handle::push(entry) }
        };
        let ret = ret?.result();

        if ret >= 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    // TODO delay_until
}
