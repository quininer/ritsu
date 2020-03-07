pub mod fs;
pub mod fs2;
pub mod timeout;
pub mod tcp;
pub mod poll;

use std::io;
use std::task::{ Context, Poll };
use futures_util::future;
use bytes::{ Bytes, BytesMut };
use crate::Handle;


pub trait AsHandle {
    type Handle: Handle;

    fn as_handle(&self) -> &Self::Handle;
}

pub trait AsyncRead {
    fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Option<BytesMut>>>;
}

pub trait AsyncWrite {
    fn push(&mut self, buf: Bytes) -> io::Result<()>;

    fn poll_flush(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>>;
}

impl dyn AsyncRead {
    pub async fn read(&mut self) -> io::Result<Option<BytesMut>> {
        future::poll_fn(|cx| self.poll_read(cx)).await
    }
}

impl dyn AsyncWrite {
    pub async fn write(&mut self, buf: Bytes) -> io::Result<usize> {
        self.push(buf)?;
        future::poll_fn(|cx| self.poll_flush(cx)).await
    }
}
