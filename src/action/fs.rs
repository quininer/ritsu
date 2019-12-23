use std::{ fs, io, mem };
use std::pin::Pin;
use std::future::Future;
use std::task::{ Context, Poll };
use std::os::unix::io::AsRawFd;
use io_uring::opcode;
use crate::{ oneshot, CompletionEntry, LocalHandle };


pub struct File {
    fd: fs::File,
    handle: LocalHandle,
}

pub enum ReadFileFuture<'a> {
    Running {
        fd: &'a File,
        bufs: Vec<libc::iovec>, // oh no
        buf: Vec<u8>,
        rx: oneshot::Receiver<CompletionEntry>
    },
    Error(io::Error),
    End
}

impl File {
    pub fn from_std(fd: fs::File, handle: LocalHandle) -> File {
        File { fd, handle }
    }


    pub fn read(&self, mut buf: Vec<u8>) -> ReadFileFuture<'_> {
        let (tx, rx) = oneshot::channel();

        let buf_ptr = unsafe { mem::transmute::<_, libc::iovec>(io::IoSliceMut::new(&mut buf)) };
        let mut bufs = vec![buf_ptr];
        let op = opcode::Target::Fd(self.fd.as_raw_fd());

        let entry = opcode::Readv::new(op, bufs.as_mut_ptr(), 1);

        match unsafe { self.handle.push(tx, entry.build()) } {
            Ok(_) => ReadFileFuture::Running { fd: self, bufs, buf, rx },
            Err(err) => ReadFileFuture::Error(err)
        }
    }
}

impl<'a> Future for ReadFileFuture<'a> {
    type Output = io::Result<Vec<u8>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match mem::replace(this, ReadFileFuture::End) {
            ReadFileFuture::Running { fd, bufs, buf, mut rx } => {
                match Pin::new(&mut rx).poll(cx) {
                    Poll::Ready(cqe) => {
                        let res = cqe.result();
                        if res >= 0 {
                            Poll::Ready(Ok(buf))
                        } else {
                            Poll::Ready(Err(io::Error::from_raw_os_error(-res)))
                        }
                    },
                    Poll::Pending => {
                        *this = ReadFileFuture::Running { fd, bufs, buf, rx };
                        Poll::Pending
                    }
                }
            }
            ReadFileFuture::Error(err) => Poll::Ready(Err(err)),
            ReadFileFuture::End => panic!()
        }
    }
}
