use std::{ io, mem };
use std::pin::Pin;
use std::future::Future;
use std::task::{ Context, Poll };
use std::os::unix::io::AsRawFd;
use bytes::{ Buf, BufMut, Bytes, BytesMut };
use io_uring::opcode::{ self, types };
use crate::action::{ AsyncRead, AsyncWrite };
use crate::{ Handle, TicketFuture };


pub struct IoInner<Fd> {
    pub fd: Fd,
    pub handle: Handle,
    pub state: IoState
}

pub enum IoState {
    Empty,
    Reading {
        buf: mem::ManuallyDrop<BytesMut>,
        wait: TicketFuture
    },
    Write {
        buf: Bytes
    },
    Writing {
        buf: mem::ManuallyDrop<Bytes>,
        wait: TicketFuture
    }
}

impl<Fd> AsyncRead for IoInner<Fd>
where
    Fd: AsRawFd,
{
    fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<BytesMut>> {
        match mem::replace(&mut self.state, IoState::Empty) {
            IoState::Empty => {
                let buf = {
                    // TODO use bufffer allocator
                    let mut buf = BytesMut::new();
                    buf.reserve(4 * 1024);
                    buf
                };
                let mut buf = mem::ManuallyDrop::new(buf);

                let bytes = buf.bytes_mut();
                let entry = opcode::Read::new(
                    types::Target::Fd(self.fd.as_raw_fd()),
                    bytes.as_mut_ptr() as *mut _,
                    bytes.len() as _
                )
                    .build();

                let wait = unsafe { self.handle.push(entry)? };

                self.state = IoState::Reading { buf, wait };

                self.poll_read(cx)
            },
            IoState::Reading { buf, mut wait } => match Pin::new(&mut wait).poll(cx) {
                Poll::Ready(ret) => {
                    let mut buf = mem::ManuallyDrop::into_inner(buf);
                    let ret = ret.result();

                    Poll::Ready(if ret >= 0 {
                        unsafe {
                            buf.advance_mut(ret as _);
                        }

                        Ok(buf)
                    } else {
                        Err(io::Error::from_raw_os_error(-ret))
                    })
                },
                Poll::Pending => {
                    self.state = IoState::Reading { buf, wait };
                    Poll::Pending
                }
            }
            unexpected_state => {
                self.state = unexpected_state;
                Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Unexpected read operation")))
            }
        }
    }
}

impl<Fd> AsyncWrite for IoInner<Fd>
where
    Fd: AsRawFd,
{
    fn submit(&mut self, buf: Bytes) -> io::Result<()> {
        match mem::replace(&mut self.state, IoState::Empty) {
            IoState::Empty | IoState::Write { .. } => {
                self.state = IoState::Write { buf };
                Ok(())
            },
            unexpected_state => {
                self.state = unexpected_state;
                Err(io::Error::new(io::ErrorKind::Other, "A read or write operation is in progress"))
            }
        }
    }

    fn poll_flush(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Bytes>> {
        match mem::replace(&mut self.state, IoState::Empty) {
            IoState::Empty =>
                Poll::Ready(Err(io::Error::new(io::ErrorKind::WriteZero, "No writable data"))),
            IoState::Write { buf } => {
                let buf = mem::ManuallyDrop::new(buf);
                let entry = opcode::Write::new(
                    types::Target::Fd(self.fd.as_raw_fd()),
                    buf.as_ptr() as *const _,
                    buf.len() as _
                )
                    .build();

                let wait = unsafe { self.handle.push(entry)? };
                self.state = IoState::Writing { buf, wait };
                self.poll_flush(cx)
            },
            IoState::Writing { buf, mut wait } => match Pin::new(&mut wait).poll(cx) {
                Poll::Ready(ret) => {
                    let mut buf = mem::ManuallyDrop::into_inner(buf);
                    let ret = ret.result();

                    Poll::Ready(if ret >= 0 {
                        buf.advance(ret as _);
                        Ok(buf)
                    } else {
                        Err(io::Error::from_raw_os_error(-ret))
                    })
                },
                Poll::Pending => {
                    self.state = IoState::Writing { buf, wait };
                    Poll::Pending
                }
            },
            unexpected_state => {
                self.state = unexpected_state;
                Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "Unexpected write operation")))
            }
        }
    }
}
