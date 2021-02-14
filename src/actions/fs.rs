use std::io;
use std::fs::File;
use std::os::unix::io::AsRawFd;
use bytes::{ BufMut, BytesMut };
use io_uring::{ types, opcode };
use crate::handle::Handle;
use crate::actions::action;


pub async fn read_buf(handle: &dyn Handle, fd: File, mut buf: BytesMut) -> (File, io::Result<BytesMut>) {
    let uninit_buf = buf.chunk_mut();

    let read_e = opcode::Read::new(
        types::Fd(fd.as_raw_fd()),
        uninit_buf.as_mut_ptr(),
        uninit_buf.len() as _
    )
        .offset(-1)
        .build();

    drop(uninit_buf);

    let ((fd, mut buf), cqe) = unsafe {
        action(handle, (fd, buf), read_e).await
    };

    let ret = cqe.result();
    let ret = if ret >= 0 {
        unsafe {
            buf.advance_mut(ret as _);
        }

        Ok(buf)
    } else {
        Err(io::Error::from_raw_os_error(-ret))
    };

    (fd, ret)
}
