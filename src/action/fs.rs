use std::{ fs, io };
use std::marker::Unpin;
use std::os::unix::io::AsRawFd;
use bytes::{ Buf, BufMut };
use io_uring::opcode::{ self, types };
use crate::util::{ iovecs, iovecs_mut };
use crate::Handle;


pub struct File<H: Handle> {
    fd: fs::File,
    handle: H,
}

impl<H: Handle> File<H> {
    pub fn from_std(fd: fs::File, handle: H) -> File<H> {
        File { fd, handle }
    }

    pub async fn read_at<B: BufMut + Unpin + 'static>(&mut self, offset: i64, mut buf: B) -> io::Result<B> {
        let mut bufs = iovecs_mut(&mut buf);

        let op = types::Target::Fd(self.fd.as_raw_fd());
        let entry = opcode::Readv::new(op, bufs.as_mut_ptr(), bufs.len() as _)
            .offset(offset)
            .build();

        let wait = unsafe { self.handle.push(entry) };
        let ret = wait?.await.result();
        if ret >= 0 {
            drop(bufs);

            unsafe {
                buf.advance_mut(ret as _);
            }

            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    pub async fn write<B: Buf + Unpin + 'static>(&mut self, offset: i64, mut buf: B) -> io::Result<B> {
        let mut bufs = iovecs(&buf);

        let op = types::Target::Fd(self.fd.as_raw_fd());
        let entry = opcode::Writev::new(op, bufs.as_mut_ptr(), bufs.len() as _)
            .offset(offset)
            .build();

        let wait = unsafe { self.handle.push(entry) };
        let ret = wait?.await.result();
        if ret >= 0 {
            drop(bufs);
            buf.advance(ret as _);
            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    pub async fn fsync(&mut self) -> io::Result<()> {
        let op = types::Target::Fd(self.fd.as_raw_fd());
        let entry = opcode::Fsync::new(op)
            .build();

        let wait = unsafe { self.handle.push(entry) };
        let ret = wait?.await.result();
        if ret >= 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    // TODO fdatasync/sync_file_range
}
