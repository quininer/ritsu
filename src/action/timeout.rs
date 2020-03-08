use std::io;
use std::time::Duration;
use slab::Slab;
use io_uring::opcode::{ self, types };
use crate::Handle;


pub struct Timer<H: Handle> {
    timespec: Slab<types::Timespec>,
    handle: H
}

impl<H: Handle> Timer<H> {
    pub fn new(handle: H) -> Timer<H> {
        Timer {
            timespec: Slab::new(),
            handle
        }
    }

    pub async fn delay_for(&mut self, dur: Duration) -> io::Result<()> {
        let timespec = types::Timespec {
            tv_sec: dur.as_secs() as _,
            tv_nsec: dur.subsec_nanos() as _
        };
        let entry = self.timespec.vacant_entry();
        let key = entry.key();
        let timespec = entry.insert(timespec);

        let entry = opcode::Timeout::new(timespec).build();
        let wait = unsafe { self.handle.push(entry) };
        let ret = wait?.await.result();

        self.timespec.remove(key);
        if ret >= 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    // TODO delay_until
}
