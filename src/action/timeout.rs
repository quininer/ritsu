use std::io;
use std::time::Duration;
use io_uring::opcode::{ self, types };
use crate::Handle;


pub struct Timer<H: Handle> {
    timespec: Box<types::Timespec>,
    handle: H
}

impl<H: Handle> Timer<H> {
    pub fn new(handle: H) -> Timer<H> {
        Timer {
            timespec: Box::new(types::Timespec {
                tv_sec: 0,
                tv_nsec: 0
            }),
            handle
        }
    }

    pub async fn delay_for(&mut self, dur: Duration) -> io::Result<()> {
        self.timespec.tv_sec = dur.as_secs() as _;
        self.timespec.tv_nsec = dur.subsec_nanos() as _;

        let entry = opcode::Timeout::new(&*self.timespec).build();

        let wait = unsafe { self.handle.push(entry) };
        let ret = wait?.await.result();
        if ret >= 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    // TODO delay_until
}
