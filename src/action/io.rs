use std::io as std_io;
use std::cmp;
use std::pin::Pin;
use std::marker::Unpin;
use std::future::Future;
use std::task::{ Context, Poll };
use std::os::unix::io::AsRawFd;
use bytes::{ Buf, BufMut };
use tokio::io::{ AsyncRead, AsyncWrite, AsyncBufRead };
use futures_util::ready;
use io_uring::opcode::{ self, types };
use crate::util::{ ioret, MaybeLock, Buffer };
use crate::{ handle, TicketFuture };


pub struct AsyncBufIo<T> {
    fd: T,

    readfut: Option<TicketFuture>,
    readbuf: MaybeLock<Buffer>,
    eof: bool,

    writefut: Option<TicketFuture>,
    writebuf: MaybeLock<Buffer>,
    closefut: Option<TicketFuture>
}

pub struct ReadHalfRef<'a, T> {
    fd: &'a T,
    readfut: &'a mut Option<TicketFuture>,
    readbuf: &'a mut MaybeLock<Buffer>,
    eof: &'a mut bool
}

pub struct WriteHalfRef<'a, T> {
    fd: &'a T,
    writefut: &'a mut Option<TicketFuture>,
    writebuf: &'a mut MaybeLock<Buffer>,
    closefut: &'a mut Option<TicketFuture>
}

impl<T: AsRawFd + Unpin> AsyncBufIo<T> {
    pub fn new(fd: T) -> AsyncBufIo<T> {
        Self::with_capacity(fd, 4 * 1024)
    }

    pub fn with_capacity(fd: T, cap: usize) -> AsyncBufIo<T> {
        AsyncBufIo {
            fd,
            readfut: None,
            readbuf: MaybeLock::new(Buffer::new(cap)),
            eof: false,
            writefut: None,
            writebuf: MaybeLock::new(Buffer::new(cap)),
            closefut: None
        }
    }

    pub fn split_ref(&mut self) -> (ReadHalfRef<'_, T>, WriteHalfRef<'_, T>) {
        let rh = ReadHalfRef {
            fd: &self.fd,
            readfut: &mut self.readfut,
            readbuf: &mut self.readbuf,
            eof: &mut self.eof
        };
        let wh = WriteHalfRef {
            fd: &self.fd,
            writefut: &mut self.writefut,
            writebuf: &mut self.writebuf,
            closefut: &mut self.closefut
        };
        (rh, wh)
    }

    #[inline]
    fn as_read(&mut self) -> ReadHalfRef<'_, T> {
        ReadHalfRef {
            fd: &self.fd,
            readfut: &mut self.readfut,
            readbuf: &mut self.readbuf,
            eof: &mut self.eof
        }
    }

    #[inline]
    fn as_write(&mut self) -> WriteHalfRef<'_, T> {
        WriteHalfRef {
            fd: &self.fd,
            writefut: &mut self.writefut,
            writebuf: &mut self.writebuf,
            closefut: &mut self.closefut
        }
    }
}

impl<T: AsRawFd + Unpin> AsyncBufRead for ReadHalfRef<'_, T> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std_io::Result<&[u8]>> {
        let this = self.get_mut();

        if *this.eof {
            return Poll::Ready(Ok(&[]))
        }

        if let Some(mut fut) = this.readfut.take() {
            let cqe = match Pin::new(&mut fut).poll(cx) {
                Poll::Ready(cqe) => cqe,
                Poll::Pending => {
                    *this.readfut = Some(fut);
                    return Poll::Pending
                }
            };

            this.readbuf.end();

            match ioret(cqe.result())? {
                0 => {
                    *this.eof = true;
                    return Poll::Ready(Ok(&[]))
                },
                n => unsafe {
                    this.readbuf.advance_mut(n as usize);
                }
            }
        }

        if !this.readbuf.has_remaining() {
            this.readbuf.clear();

            let bytes = this.readbuf.bytes_mut();
            let entry = opcode::Read::new(
                types::Target::Fd(this.fd.as_raw_fd()),
                bytes.as_mut_ptr() as *mut _,
                bytes.len() as _
            )
                .build();

            this.readbuf.start();

            match unsafe { handle::push(entry) } {
                Ok(fut) => *this.readfut = Some(fut),
                Err(err) => {
                    this.readbuf.end();
                    return Poll::Ready(Err(err))
                }
            }

            Pin::new(this).poll_fill_buf(cx)
        } else {
            Poll::Ready(Ok(this.readbuf.bytes()))
        }
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        self.readbuf.advance(amt);
    }
}

impl<T: AsRawFd + Unpin> AsyncRead for ReadHalfRef<'_, T> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std_io::Result<usize>> {
        let rem = ready!(self.as_mut().poll_fill_buf(cx))?;

        let len = cmp::min(rem.len(), buf.len());
        buf[..len].copy_from_slice(&rem[..len]);
        self.consume(len);

        Poll::Ready(Ok(len))
    }
}

impl<T: AsRawFd + Unpin> AsyncWrite for WriteHalfRef<'_, T> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std_io::Result<usize>> {
        let this = self.get_mut();

        if !this.writebuf.has_remaining_mut() || this.writefut.is_some() {
            ready!(Pin::new(&mut *this).poll_flush(cx))?;
        }

        let len = cmp::min(this.writebuf.remaining_mut(), buf.len());
        this.writebuf.put_slice(&buf[..len]);

        // flush if full
        if !this.writebuf.has_remaining_mut() {
            let bytes = this.writebuf.bytes();
            let entry = opcode::Write::new(
                types::Target::Fd(this.fd.as_raw_fd()),
                bytes.as_ptr() as *const _,
                bytes.len() as _
            )
                .build();

            this.writebuf.start();

            match unsafe { handle::push(entry) } {
                Ok(fut) => *this.writefut = Some(fut),
                Err(err) => {
                    this.writebuf.end();
                    return Poll::Ready(Err(err))
                }
            }
        }

        Poll::Ready(Ok(len))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<std_io::Result<()>> {
        let this = self.get_mut();

        while this.writebuf.has_remaining() {
            if let Some(mut fut) = this.writefut.take() {
                let cqe = match Pin::new(&mut fut).poll(cx) {
                    Poll::Ready(cqe) => cqe,
                    Poll::Pending => {
                        *this.writefut = Some(fut);
                        return Poll::Pending
                    }
                };

                this.writebuf.end();

                let n = ioret(cqe.result())?;
                this.writebuf.advance(n as usize);
            }

            let bytes = this.writebuf.bytes();
            let entry = opcode::Write::new(
                types::Target::Fd(this.fd.as_raw_fd()),
                bytes.as_ptr() as *const _,
                bytes.len() as _
            )
                .build();

            this.writebuf.start();

            match unsafe { handle::push(entry) } {
                Ok(fut) => *this.writefut = Some(fut),
                Err(err) => {
                    this.writebuf.end();
                    return Poll::Ready(Err(err))
                }
            }
        }

        this.writebuf.clear();

        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<std_io::Result<()>> {
        let this = self.get_mut();

        if let Some(mut fut) = this.writefut.take() {
            let cqe = match Pin::new(&mut fut).poll(cx) {
                Poll::Ready(cqe) => cqe,
                Poll::Pending => {
                    *this.writefut = Some(fut);
                    return Poll::Pending
                }
            };

            this.writebuf.end();

            let n = ioret(cqe.result())?;
            this.writebuf.advance(n as usize);
        }

        if let Some(mut fut) = this.closefut.take() {
            let cqe = match Pin::new(&mut fut).poll(cx) {
                Poll::Ready(cqe) => cqe,
                Poll::Pending => {
                    *this.closefut = Some(fut);
                    return Poll::Pending
                }
            };

            ioret(cqe.result())?;

            Poll::Ready(Ok(()))
        } else {
            let entry = opcode::Close::new(this.fd.as_raw_fd())
                .build();

            *this.closefut = Some(unsafe { handle::push(entry)? });

            Poll::Pending
        }
    }
}

impl<T: AsRawFd + Unpin> AsyncBufRead for AsyncBufIo<T> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std_io::Result<&[u8]>> {
        let mut rh = self.get_mut().as_read();
        ready!(Pin::new(&mut rh).poll_fill_buf(cx))?;
        Poll::Ready(Ok(rh.readbuf.bytes()))
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        self.readbuf.advance(amt);
    }
}

impl<T: AsRawFd + Unpin> AsyncRead for AsyncBufIo<T> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std_io::Result<usize>> {
        Pin::new(&mut self.as_read()).poll_read(cx, buf)
    }
}

impl<T: AsRawFd + Unpin> AsyncWrite for AsyncBufIo<T> {
    #[inline]
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std_io::Result<usize>> {
        Pin::new(&mut self.as_write()).poll_write(cx, buf)
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<std_io::Result<()>> {
        Pin::new(&mut self.as_write()).poll_flush(cx)
    }

    #[inline]
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<std_io::Result<()>> {
        Pin::new(&mut self.as_write()).poll_shutdown(cx)
    }
}
