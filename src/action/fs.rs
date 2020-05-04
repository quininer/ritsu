use std::{ fs, io };
use std::os::unix::io::{ AsRawFd, RawFd };
use bytes::{ Buf, BufMut, Bytes, BytesMut };
use io_uring::opcode::{ self, types };
use crate::handle;


pub struct File {
    fd: fs::File
}

impl File {
    pub fn from_std(fd: fs::File) -> File {
        File { fd }
    }

    pub async fn read_at(&mut self, offset: i64, mut buf: BytesMut) -> io::Result<BytesMut> {
        let bytes = buf.bytes_mut();
        let entry = opcode::Read::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            bytes.as_mut_ptr() as *mut _,
            bytes.len() as _
        )
            .offset(offset)
            .build();

        let ret = safety_await!{
            [ buf ];
            unsafe { handle::push(entry) }
        };

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

    pub async fn write_at(&mut self, offset: i64, mut buf: Bytes) -> io::Result<Bytes> {
        let entry = opcode::Write::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            buf.as_ptr() as *const _,
            buf.len() as _
        )
            .offset(offset)
            .build();

        let ret = safety_await!{
            [ buf ];
            unsafe { handle::push(entry) }
        };
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

        let ret = safety_await!{
            unsafe { handle::push(entry) }
        };
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

impl AsRawFd for File {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
