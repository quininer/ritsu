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

pub enum ReadFileFuture {
    Running {
        fd: File,
        bufs: Vec<libc::iovec>,
        buf: Vec<u8>,
        rx: oneshot::Receiver<CompletionEntry>
    },
    Error { fd: File, err: io::Error },
    End
}

impl File {
    pub fn from_std(fd: fs::File, handle: LocalHandle) -> File {
        File { fd, handle }
    }
}

pub fn read(fd: File, mut buf: Vec<u8>) -> ReadFileFuture {
    let (tx, rx) = oneshot::channel();

    let buf_ptr = unsafe { mem::transmute::<_, libc::iovec>(io::IoSliceMut::new(&mut buf)) };
    let mut bufs = vec![buf_ptr];
    let op = opcode::Target::Fd(fd.fd.as_raw_fd());

    let entry = opcode::Readv::new(op, bufs.as_mut_ptr(), 1);

    match unsafe { fd.handle.push(tx, entry.build()) } {
        Ok(_) => ReadFileFuture::Running { fd, bufs, buf, rx },
        Err(err) => ReadFileFuture::Error { fd, err }
    }
}

impl Future for ReadFileFuture {
    type Output = (File, io::Result<Vec<u8>>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match mem::replace(this, ReadFileFuture::End) {
            ReadFileFuture::Running { fd, bufs, buf, mut rx } => {
                match Pin::new(&mut rx).poll(cx) {
                    Poll::Ready(cqe) => {
                        let res = cqe.result();
                        if res >= 0 {
                            Poll::Ready((fd, Ok(buf)))
                        } else {
                            Poll::Ready((fd, Err(io::Error::from_raw_os_error(-res))))
                        }
                    },
                    Poll::Pending => {
                        *this = ReadFileFuture::Running { fd, bufs, buf, rx };
                        Poll::Pending
                    }
                }
            }
            ReadFileFuture::Error { fd, err } => Poll::Ready((fd, Err(err))),
            ReadFileFuture::End => panic!()
        }
    }
}
