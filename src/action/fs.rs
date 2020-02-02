use std::{ fs, io };
use std::marker::Unpin;
use std::os::unix::io::AsRawFd;
use bytes::{ Buf, BufMut, buf::IoSliceMut };
use io_uring::opcode::{ self, types };
use crate::Handle;


pub struct File<H: Handle> {
    fd: fs::File,
    offset: i64,
    handle: H,
}

impl<H: Handle> File<H> {
    pub fn from_std(fd: fs::File, handle: H) -> File<H> {
        File { fd, handle, offset: 0 }
    }

    pub async fn read<B: BufMut + Unpin + 'static>(&mut self, mut buf: B) -> io::Result<B> {
        let mut bufs: Vec<libc::iovec> = unsafe {
            let mut bufs: Vec<IoSliceMut> = Vec::with_capacity(32);
            bufs.set_len(bufs.capacity());

            let n = buf.bytes_vectored_mut(&mut bufs);
            bufs.set_len(n);

            let (ptr, len, cap) = bufs.into_raw_parts();
            Vec::from_raw_parts(ptr as *mut _, len, cap)
        };

        let wait = unsafe {
            let op = types::Target::Fd(self.fd.as_raw_fd());
            let entry = opcode::Readv::new(op, bufs.as_mut_ptr(), bufs.len() as _)
                .offset(self.offset)
                .build();
            self.handle.push(entry)?
        };

        let res = wait.await.result();
        drop(bufs);
        if res >= 0 {
            unsafe {
                buf.advance_mut(res as _);
            }

            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-res))
        }
    }

    pub async fn write<B: Buf + Unpin + 'static>(&mut self, mut buf: B) -> io::Result<B> {
        let mut bufs: Vec<libc::iovec> = unsafe {
            let mut bufs: Vec<io::IoSlice> = Vec::with_capacity(32);
            bufs.set_len(bufs.capacity());

            let n = buf.bytes_vectored(&mut bufs);
            bufs.set_len(n);

            let (ptr, len, cap) = bufs.into_raw_parts();
            Vec::from_raw_parts(ptr as *mut _, len, cap)
        };

        let wait = unsafe {
            let op = types::Target::Fd(self.fd.as_raw_fd());
            let entry = opcode::Writev::new(op, bufs.as_mut_ptr(), bufs.len() as _)
                .offset(self.offset)
                .build();
            self.handle.push(entry)?
        };

        let res = wait.await.result();
        drop(bufs);
        if res >= 0 {
            buf.advance(res as _);
            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-res))
        }
    }
}
