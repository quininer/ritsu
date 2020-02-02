use std::{ fs, io, mem };
use std::pin::Pin;
use std::marker::Unpin;
use std::future::Future;
use std::task::{ Context, Poll };
use std::os::unix::io::AsRawFd;
use bytes::{ BufMut, buf::IoSliceMut };
use io_uring::opcode::{ self, types };
use crate::Handle;


pub struct File<H: Handle> {
    fd: fs::File,
    offset: i64,
    handle: H,
}

pub struct ReadFuture<'a, H: Handle, B>(Inner<'a, H, B>);

enum Inner<'a, H: Handle,B> {
    Running {
        fd: &'a File<H>,
        bufs: Vec<libc::iovec>,
        buf: B,
        rx: H::Wait
    },
    Error(io::Error),
    End
}

impl<H: Handle> File<H> {
    pub fn from_std(fd: fs::File, handle: H) -> File<H> {
        File { fd, handle, offset: 0 }
    }

    pub fn read<B: BufMut + Unpin + 'static>(&self, mut buf: B) -> ReadFuture<'_, H, B> {
        let mut bufs: Vec<libc::iovec> = unsafe {
            let mut bufs: Vec<IoSliceMut> = Vec::with_capacity(32);
            bufs.set_len(bufs.capacity());

            let n = buf.bytes_vectored_mut(&mut bufs);
            bufs.set_len(n);

            let (ptr, len, cap) = bufs.into_raw_parts();
            Vec::from_raw_parts(ptr as *mut _, len, cap)
        };

        let op = types::Target::Fd(self.fd.as_raw_fd());
        let entry = opcode::Readv::new(op, bufs.as_mut_ptr(), bufs.len() as _)
            .offset(self.offset)
            .build();

        ReadFuture(match unsafe { self.handle.push(entry) } {
            Ok(rx) => Inner::Running { fd: self, bufs, buf, rx },
            Err(err) => Inner::Error(err)
        })
    }
}

impl<'a, H, B> Future for ReadFuture<'a, H, B>
where
    H: Handle,
    H::Wait: Unpin,
    B: BufMut + Unpin + 'static
{
    type Output = io::Result<B>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match mem::replace(&mut self.0, Inner::End) {
            Inner::Running { fd, bufs, mut buf, mut rx } => {
                match Pin::new(&mut rx).poll(cx) {
                    Poll::Ready(cqe) => {
                        let res = cqe.result();
                        if res >= 0 {
                            unsafe {
                                buf.advance_mut(res as _);
                            }

                            Poll::Ready(Ok(buf))
                        } else {
                            Poll::Ready(Err(io::Error::from_raw_os_error(-res)))
                        }
                    },
                    Poll::Pending => {
                        self.0 = Inner::Running { fd, bufs, buf, rx };
                        Poll::Pending
                    }
                }
            }
            Inner::Error(err) => Poll::Ready(Err(err)),
            Inner::End => panic!()
        }
    }
}
