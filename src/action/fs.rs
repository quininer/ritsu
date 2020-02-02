use std::{ fs, io, mem };
use std::pin::Pin;
use std::marker::Unpin;
use std::future::Future;
use std::task::{ Context, Poll };
use std::os::unix::io::AsRawFd;
use io_uring::opcode::{ self, types };
use crate::Handle;


pub struct File<H: Handle> {
    fd: fs::File,
    offset: i64,
    handle: H,
}

pub struct ReadFuture<'a, H: Handle>(Inner<'a, H>);

alloc!(
    static BUF_ONE = IoVecAlloc<[libc::iovec; 1]> as AllocKey;
);

enum Inner<'a, H: Handle> {
    Running {
        fd: &'a File<H>,
        bufkey: AllocKey,
        buf: Vec<u8>,
        rx: H::Wait
    },
    Error(io::Error),
    End
}

impl<H: Handle> File<H> {
    pub fn from_std(fd: fs::File, handle: H) -> File<H> {
        File { fd, handle, offset: 0 }
    }

    pub fn read(&self, mut buf: Vec<u8>) -> ReadFuture<'_, H> {
        let bufptr =
            unsafe { mem::transmute::<_, libc::iovec>(io::IoSliceMut::new(&mut buf)) };
        let bufptr = [bufptr];

        let (bufkey, entry) = BUF_ONE.with(|alloc| alloc.alloc(bufptr, |bufptr| {
            let op = types::Target::Fd(self.fd.as_raw_fd());
            opcode::Readv::new(op, bufptr.as_mut_ptr(), 1).offset(self.offset)
        }));

        ReadFuture(match unsafe { self.handle.push(entry.build()) } {
            Ok(rx) => Inner::Running { fd: self, bufkey, buf, rx },
            Err(err) => Inner::Error(err)
        })
    }
}

impl<'a, H> Future for ReadFuture<'a, H>
where
    H: Handle,
    H::Wait: Unpin
{
    type Output = io::Result<Vec<u8>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match mem::replace(&mut self.0, Inner::End) {
            Inner::Running { fd, bufkey, buf, mut rx } => {
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
                        self.0 = Inner::Running { fd, bufkey, buf, rx };
                        Poll::Pending
                    }
                }
            }
            Inner::Error(err) => Poll::Ready(Err(err)),
            Inner::End => panic!()
        }
    }
}
