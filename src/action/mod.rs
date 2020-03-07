pub mod fs;
pub mod fs2;
pub mod timeout;
pub mod tcp;
pub mod poll;

use std::io;
use std::task::{ Context, Poll };
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
