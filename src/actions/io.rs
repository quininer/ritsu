use std::io;
use std::os::unix::io::AsRawFd;
use bytes::{ Buf, BufMut };
use io_uring::{ types, opcode };
use crate::handle::Handle;
use crate::actions::{ action, PushError };


pub unsafe trait TrustedAsRawFd: AsRawFd + 'static {}

unsafe impl TrustedAsRawFd for std::fs::File {}
unsafe impl TrustedAsRawFd for std::io::Stdout {}
unsafe impl TrustedAsRawFd for std::io::Stderr {}
unsafe impl TrustedAsRawFd for std::net::TcpStream {}


pub async fn read_buf<H: Handle, T: TrustedAsRawFd, B: BufMut + 'static>(
    handle: H,
    fd: &mut Option<T>,
    mut buf: B,
    offset: Option<u32>
)
    -> io::Result<(T, B)>
{
    let fd2 = match fd.take() {
        Some(fd) => fd,
        None => return Err(not_found())
    };

    let chunk = buf.chunk_mut();

    let read_e = opcode::Read::new(
        types::Fd(fd2.as_raw_fd()),
        chunk.as_mut_ptr(),
        chunk.len() as _
    )
        .offset(offset.map(|offset| offset as _).unwrap_or(-1))
        .build();

    let ((fd2, mut buf), cqe) = unsafe {
        action(handle, (fd2, buf), read_e)
            .map_err(PushError::into_error)?.await
    };

    let ret = cqe.result();
    if ret >= 0 {
        unsafe {
            buf.advance_mut(ret as _);
        }

        Ok((fd2, buf))
    } else {
        *fd = Some(fd2);
        Err(io::Error::from_raw_os_error(-ret))
    }
}

pub async fn write_buf<H: Handle, T: TrustedAsRawFd, B: Buf + 'static>(
    handle: H,
    fd: &mut Option<T>,
    buf: B,
    offset: Option<u32>
)
    -> io::Result<(T, B)>
{
    let fd2 = match fd.take() {
        Some(fd) => fd,
        None => return Err(not_found())
    };

    let chunk = buf.chunk();

    let write_e = opcode::Write::new(
        types::Fd(fd2.as_raw_fd()),
        chunk.as_ptr(),
        chunk.len() as _
    )
        .offset(offset.map(|offset| offset as _).unwrap_or(-1))
        .build();

    let ((fd2, mut buf), cqe) = unsafe {
        action(handle, (fd2, buf), write_e)
            .map_err(PushError::into_error)?.await
    };

    let ret = cqe.result();
    if ret >= 0 {
        buf.advance(ret as _);

        Ok((fd2, buf))
    } else {
        *fd = Some(fd2);
        Err(io::Error::from_raw_os_error(-ret))
    }
}

#[cold]
fn not_found() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        "No available fd was found"
    )
}
