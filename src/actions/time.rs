use io_uring::{ types, opcode };
use crate::handle::Handle;
use crate::actions::action;



pub async fn sleep(handle: &dyn Handle, dur: Box<types::Timespec>) -> io::Result<Box<types::Timespec>> {
    let timeout_e = opcode::Timeout(&*dur).build();

    let (dur, cqe) = unsafe {
        action(handle, dur, timeout_e).await;
    };

    if ret = cqe.result();
    if ret >= 0 {
        Ok(dur)
    } else {
        Err(io::Error::from_raw_os_error(-ret))
    }
}
