use std::{ fs, io, mem };
use std::pin::Pin;
use std::future::Future;
use std::task::{ Context, Poll };
use std::os::unix::io::AsRawFd;
use bytes::{ BufMut, Bytes, BytesMut };
use io_uring::opcode::{ self, types };
use crate::util::{ iovecs2, boxed_iovec_mut, IoVec };
use crate::Handle;
use crate::action::{ AsyncRead, AsyncWrite };


pub struct File<H: Handle> {
    fd: fs::File,
    handle: H,
    offset: i64,
    state: State<H>
}

enum State<H: Handle> {
    Empty,
    Reading {
        buf: mem::ManuallyDrop<BytesMut>,
        iovec: mem::ManuallyDrop<Box<IoVec>>,
        wait: H::Wait
    },
    Write {
        bufs: Vec<Bytes>
    },
    Writing {
        bufs: mem::ManuallyDrop<Vec<Bytes>>,
        iovec: mem::ManuallyDrop<Vec<IoVec>>,
        wait: H::Wait
    }
}

impl<H: Handle> File<H> {
    pub fn from_std(fd: fs::File, handle: H) -> File<H> {
        File {
            fd, handle,
            offset: 0,
            state: State::Empty
        }
    }
}

impl<H: Handle> AsyncRead for File<H> {
    fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Option<BytesMut>>> {
        self.state = match mem::replace(&mut self.state, State::Empty) {
            State::Empty => {
                let mut buf = BytesMut::new();
                buf.reserve(4 * 1024);
                let iovec = boxed_iovec_mut(&mut buf);

                let op = types::Target::Fd(self.fd.as_raw_fd());
                let entry = opcode::Readv::new(op, iovec.as_ptr() as *mut libc::iovec, 1)
                    .offset(self.offset)
                    .build();

                let wait = unsafe { self.handle.push(entry)? };

                State::Reading {
                    buf: mem::ManuallyDrop::new(buf),
                    iovec: mem::ManuallyDrop::new(iovec),
                    wait
                }
            },
            State::Reading { buf, iovec, mut wait } => match Pin::new(&mut wait).poll(cx) {
                Poll::Ready(cqe) => {
                    let result = cqe.result();
                    let _ = mem::ManuallyDrop::into_inner(iovec);
                    let mut buf = mem::ManuallyDrop::into_inner(buf);

                    return if result > 0 {
                        unsafe {
                            buf.advance_mut(result as _);
                        }

                        Poll::Ready(Ok(Some(buf)))
                    } else if result == 0 {
                        Poll::Ready(Ok(None))
                    } else {
                        Poll::Ready(Err(io::Error::from_raw_os_error(-result)))
                    };
                },
                Poll::Pending => State::Reading { buf, iovec, wait }
            },
            unexpected_state => {
                self.state = unexpected_state;
                return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Unexpected read operation")));
            }
        };

        Poll::Pending
    }
}

impl<H: Handle> AsyncWrite for File<H> {
    fn push(&mut self, buf: Bytes) -> io::Result<()> {
        match &mut self.state {
            State::Empty => {
                self.state = State::Write { bufs: vec![buf] };
                Ok(())
            }
            State::Write { bufs } => {
                bufs.push(buf);
                Ok(())
            },
            _ => Err(io::Error::new(io::ErrorKind::Other, "A read or write operation is in progress"))
        }
    }

    fn poll_flush(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        self.state = match mem::replace(&mut self.state, State::Empty) {
            State::Empty => return Poll::Ready(Err(io::Error::new(io::ErrorKind::WriteZero, "No writable data"))),
            State::Write { bufs } => {
                let iovecs = iovecs2(&bufs);

                let op = types::Target::Fd(self.fd.as_raw_fd());
                let entry = opcode::Writev::new(op, iovecs.as_ptr() as *const libc::iovec, bufs.len() as _)
                    .offset(self.offset)
                    .build();

                let wait = unsafe { self.handle.push(entry)? };

                State::Writing {
                    bufs: mem::ManuallyDrop::new(bufs),
                    iovec: mem::ManuallyDrop::new(iovecs),
                    wait
                }
            },
            State::Writing { bufs, iovec, mut wait } => match Pin::new(&mut wait).poll(cx) {
                Poll::Ready(cqe) => {
                    let result = cqe.result();
                    let _ = mem::ManuallyDrop::into_inner(iovec);
                    let _ = mem::ManuallyDrop::into_inner(bufs);

                    return if result >= 0 {
                        Poll::Ready(Ok(result as _))
                    } else {
                        Poll::Ready(Err(io::Error::from_raw_os_error(-result)))
                    };
                },
                Poll::Pending => State::Writing { bufs, iovec, wait }
            },
            unexpected_state => {
                self.state = unexpected_state;
                return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Unexpected write operation")));
            }
        };

        Poll::Pending
    }
}
