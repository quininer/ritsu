pub mod iohelp;
pub mod fs;
pub mod timeout;
pub mod tcp;
pub mod poll;

use std::io;
use std::pin::Pin;
use std::future::Future;
use std::task::{ Context, Poll };
use bytes::{ Bytes, BytesMut };
use crate::Handle;


pub trait AsHandle {
    type Handle: Handle;

    fn as_handle(&self) -> &Self::Handle;
}

pub trait AsyncRead {
    fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<BytesMut>>;
}

pub trait AsyncWrite {
    fn submit(&mut self, buf: Bytes) -> io::Result<()>;

    fn poll_flush(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Bytes>>;
}

pub trait AsyncReadExt: AsyncRead {
    fn read(&mut self) -> ReadFuture<'_>;
}

pub trait AsyncWriteExt: AsyncWrite {
    fn write(&mut self, buf: Bytes) -> WriteFuture<'_>;
}

impl<R: AsyncRead> AsyncReadExt for R {
    fn read(&mut self) -> ReadFuture<'_> {
        ReadFuture(self)
    }
}

impl<W: AsyncWrite> AsyncWriteExt for W {
    fn write(&mut self, buf: Bytes) -> WriteFuture<'_> {
        match self.submit(buf) {
            Ok(()) => WriteFuture(Ok(self)),
            Err(err) => WriteFuture(Err(Some(err)))
        }
    }
}

pub struct ReadFuture<'a>(&'a mut dyn AsyncRead);

impl Future for ReadFuture<'_> {
    type Output = io::Result<BytesMut>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_read(cx)
    }
}

pub struct WriteFuture<'a>(Result<&'a mut dyn AsyncWrite, Option<io::Error>>);

impl Future for WriteFuture<'_> {
    type Output = io::Result<Bytes>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &mut self.0 {
            Ok(writer) => writer.poll_flush(cx),
            Err(err) => match err.take() {
                Some(err) => Poll::Ready(Err(err)),
                None => panic!()
            }
        }
    }
}
