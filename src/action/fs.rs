use std::{ fs, io, mem };
use std::task::{ Context, Poll };
use std::os::unix::io::{ AsRawFd, RawFd };
use bytes::{ Buf, BufMut, Bytes, BytesMut };
use io_uring::opcode::{ self, types };
use crate::Handle;
use crate::action::{ AsHandle, AsyncRead, AsyncWrite };
use crate::action::iohelp::{ IoInner, IoState };


pub struct File<H: Handle> {
    fd: fs::File,
    handle: H
}

pub struct FileIo<H: Handle>(IoInner<fs::File, H>);

impl<H: Handle> File<H> {
    pub fn from_std(fd: fs::File, handle: H) -> File<H> {
        File { fd, handle }
    }

    pub fn into_io(self) -> FileIo<H> {
        FileIo(IoInner {
            fd: self.fd,
            state: IoState::Empty,
            handle: self.handle
        })
    }

    pub async fn read_at(&mut self, offset: i64, buf: BytesMut) -> io::Result<BytesMut> {
        let mut buf = mem::ManuallyDrop::new(buf);

        let bytes = buf.bytes_mut();
        let entry = opcode::Read::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            bytes.as_mut_ptr() as *mut _,
            bytes.len() as _
        )
            .offset(offset)
            .build();

        let ret = unsafe { self.handle.push(entry).await };
        let mut buf = mem::ManuallyDrop::into_inner(buf);
        let ret = ret?.result();

        if ret >= 0 {
            unsafe {
                buf.advance_mut(ret as _);
            }

            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    pub async fn write_at(&mut self, offset: i64, buf: Bytes) -> io::Result<Bytes> {
        let buf = mem::ManuallyDrop::new(buf);

        let entry = opcode::Write::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            buf.as_ptr() as *const _,
            buf.len() as _
        )
            .offset(offset)
            .build();

        let ret = unsafe { self.handle.push(entry).await };
        let mut buf = mem::ManuallyDrop::into_inner(buf);
        let ret = ret?.result();

        if ret >= 0 {
            buf.advance(ret as _);
            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    async fn fsync(&self, flag: types::FsyncFlags) -> io::Result<()> {
        let op = types::Target::Fd(self.fd.as_raw_fd());
        let entry = opcode::Fsync::new(op)
            .flags(flag)
            .build();

        let ret = unsafe { self.handle.push(entry).await };
        let ret = ret?.result();

        if ret >= 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    #[inline]
    pub async fn sync_all(&self) -> io::Result<()> {
        self.fsync(types::FsyncFlags::empty()).await
    }

    #[inline]
    pub async fn sync_data(&self) -> io::Result<()> {
        self.fsync(types::FsyncFlags::DATASYNC).await
    }
}

impl<H: Handle> AsyncRead for FileIo<H> {
    #[inline]
    fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<BytesMut>> {
        self.0.poll_read(cx)
    }
}

impl<H: Handle> AsyncWrite for FileIo<H> {
    #[inline]
    fn submit(&mut self, buf: Bytes) -> io::Result<()> {
        self.0.submit(buf)
    }

    #[inline]
    fn poll_flush(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Bytes>> {
        self.0.poll_flush(cx)
    }
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
