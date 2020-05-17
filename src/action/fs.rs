use std::{ fs, io };
use std::os::unix::io::{ AsRawFd, RawFd };
use bytes::{ Buf, BufMut };
use io_uring::opcode::{ self, types };
use crate::{ handle, util::ioret };


pub struct File {
    fd: fs::File
}

impl File {
    pub fn from_std(fd: fs::File) -> File {
        File { fd }
    }

    pub async fn read_at<B: BufMut + 'static>(&mut self, offset: i64, mut buf: B) -> io::Result<B> {
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

        let n = ioret(ret?.result())?;

        unsafe {
            buf.advance_mut(n as usize);
        }

        Ok(buf)
    }

    pub async fn write_at<B: Buf + 'static>(&mut self, offset: i64, mut buf: B) -> io::Result<B> {
        let bytes = buf.bytes();
        let entry = opcode::Write::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            bytes.as_ptr() as *const _,
            bytes.len() as _
        )
            .offset(offset)
            .build();

        let ret = safety_await!{
            [ buf ];
            unsafe { handle::push(entry) }
        };

        let n = ioret(ret?.result())?;
        buf.advance(n as usize);
        Ok(buf)
    }

    async fn fsync(&self, flag: types::FsyncFlags) -> io::Result<()> {
        let op = types::Target::Fd(self.fd.as_raw_fd());
        let entry = opcode::Fsync::new(op)
            .flags(flag)
            .build();

        let ret = safety_await!{
            unsafe { handle::push(entry) }
        };

        ioret(ret?.result())?;

        Ok(())
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
