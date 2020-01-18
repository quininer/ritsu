pub mod fs;

/*
use std::io;
use std::any::Any;
use std::marker::{ Unpin, PhantomData };
use bytes::{ Buf, BufMut };
use crate::{ oneshot, CompletionEntry, LocalHandle };


pub trait AsyncRead {
    async fn read<B: BufMut + Unpin + 'static>(&mut self, buf: B) -> io::Result<B>;
}

pub trait AsyncWrite {
    async fn write<B: Buf + Unpin + 'static>(&mut self, buf: B) -> io::Result<B>;
}

*/
