use std::io;
use io_uring::{ types, opcode };
use crate::handle::Handle;
use crate::actions::{ action, PushError };



pub async fn sleep<H: Handle>(handle: H, dur: Box<types::Timespec>) -> io::Result<Box<types::Timespec>> {
    let timeout_e = opcode::Timeout::new(&*dur).build();

    let (dur, cqe) = unsafe {
        action(handle, dur, timeout_e)
            .map_err(PushError::into_error)?.await
    };

    let ret = cqe.result();
    if ret >= 0 {
        Ok(dur)
    } else {
        Err(io::Error::from_raw_os_error(-ret))
    }
}
