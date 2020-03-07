use std::{ fs, io, mem };
use std::marker::Unpin;
use std::os::unix::io::{ AsRawFd, RawFd };
use bytes::{ Buf, BufMut };
use io_uring::opcode::{ self, types };
use crate::util::{ iovecs, iovecs_mut };
use crate::action::AsHandle;
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
        let entry = opcode::Readv::new(op, bufs.as_mut_ptr() as *mut libc::iovec, bufs.len() as _)
            .offset(offset)
            .build();
        let bufs = mem::ManuallyDrop::new(bufs);
        let buf = mem::ManuallyDrop::new(buf);

        let wait = unsafe { self.handle.push(entry) };
        let ret = wait?.await.result();

        let _bufs = mem::ManuallyDrop::into_inner(bufs);
        let mut buf = mem::ManuallyDrop::into_inner(buf);

        if ret >= 0 {
            unsafe {
                buf.advance_mut(ret as _);
            }

            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    pub async fn write_at<B: Buf + Unpin + 'static>(&mut self, offset: i64, buf: B) -> io::Result<B> {
        let bufs = iovecs(&buf);

        let op = types::Target::Fd(self.fd.as_raw_fd());
        let entry = opcode::Writev::new(op, bufs.as_ptr() as *const libc::iovec, bufs.len() as _)
            .offset(offset)
            .build();
        let bufs = mem::ManuallyDrop::new(bufs);
        let buf = mem::ManuallyDrop::new(buf);

        let wait = unsafe { self.handle.push(entry) };
        let ret = wait?.await.result();

        let _bufs = mem::ManuallyDrop::into_inner(bufs);
        let mut buf = mem::ManuallyDrop::into_inner(buf);

        if ret >= 0 {
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

impl<H: Handle> AsHandle for File<H> {
    type Handle = H;

    #[inline]
    fn as_handle(&self) -> &Self::Handle {
        &self.handle
    }
}

impl<H: Handle> AsRawFd for File<H> {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
