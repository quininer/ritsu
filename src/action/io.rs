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
    writefut: Option<TicketFuture>,
    writebuf: MaybeLock<Buffer>,
    closefut: Option<TicketFuture>
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
            writefut: None,
            writebuf: MaybeLock::new(Buffer::new(cap)),
            closefut: None
        }
    }
}

impl<T: AsRawFd + Unpin> AsyncBufRead for AsyncBufIo<T> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std_io::Result<&[u8]>> {
        let this = self.get_mut();

        if let Some(mut fut) = this.readfut.take() {
            let cqe = match Pin::new(&mut fut).poll(cx) {
                Poll::Ready(cqe) => cqe,
                Poll::Pending => {
                    this.readfut = Some(fut);
                    return Poll::Pending
                }
            };

            this.readbuf.end();

            match ioret(cqe.result()) {
                Ok(0) => return Poll::Ready(Ok(&[])),
                Ok(n) => unsafe {
                    this.readbuf.advance_mut(n as usize);
                },
                Err(err) => return Poll::Ready(Err(err))
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
                Ok(fut) => this.readfut = Some(fut),
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

impl<T: AsRawFd + Unpin> AsyncRead for AsyncBufIo<T> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std_io::Result<usize>> {
        let rem = ready!(self.as_mut().poll_fill_buf(cx))?;
        let len = cmp::min(rem.len(), buf.len());
        buf[..len].copy_from_slice(&rem[..len]);
        self.consume(len);
        Poll::Ready(Ok(len))
    }
}

impl<T: AsRawFd + Unpin> AsyncWrite for AsyncBufIo<T> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std_io::Result<usize>> {
        let this = self.get_mut();

        if !this.writebuf.has_remaining_mut() || this.writefut.is_some() {
            ready!(Pin::new(&mut *this).poll_flush(cx))?;
        }

        let len = cmp::min(this.writebuf.remaining_mut(), buf.len());
        this.writebuf.put_slice(&buf[..len]);

        Poll::Ready(Ok(len))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<std_io::Result<()>> {
        let this = self.get_mut();

        while this.writebuf.has_remaining() {
            if let Some(mut fut) = this.writefut.take() {
                let cqe = match Pin::new(&mut fut).poll(cx) {
                    Poll::Ready(cqe) => cqe,
                    Poll::Pending => {
                        this.writefut = Some(fut);
                        return Poll::Pending
                    }
                };

                this.writebuf.end();

                match ioret(cqe.result()) {
                    Ok(n) => this.writebuf.advance(n as usize),
                    Err(err) => return Poll::Ready(Err(err))
                }
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
                Ok(fut) => this.writefut = Some(fut),
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
                    this.writefut = Some(fut);
                    return Poll::Pending
                }
            };

            this.writebuf.end();

            match ioret(cqe.result()) {
                Ok(n) => this.writebuf.advance(n as usize),
                Err(err) => return Poll::Ready(Err(err))
            }
        }

        if let Some(mut fut) = this.closefut.take() {
            let cqe = match Pin::new(&mut fut).poll(cx) {
                Poll::Ready(cqe) => cqe,
                Poll::Pending => {
                    this.closefut = Some(fut);
                    return Poll::Pending
                }
            };

            ioret(cqe.result())?;

            Poll::Ready(Ok(()))
        } else {
            let entry = opcode::Close::new(this.fd.as_raw_fd())
                .build();

            this.closefut = Some(unsafe { handle::push(entry)? });

            Poll::Pending
        }
    }
}
