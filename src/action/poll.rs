use std::io;
use std::pin::Pin;
use std::marker::Unpin;
use std::task::{ Context, Poll as TaskPoll };
use std::future::Future;
use std::os::unix::io::{ AsRawFd, RawFd };
use bitflags::bitflags;
use futures_util::ready;
use io_uring::opcode::{ self, types };
use crate::action::AsHandle;
use crate::Handle;


bitflags!{
    pub struct Poll: i16 {
        const READABLE = libc::POLLIN;
        const WRITABLE = libc::POLLOUT;

        // TODO
    }
}

pub trait ReadyExt: AsHandle + AsRawFd {
    fn ready(&self, poll: Poll) -> ReadyFuture<Self::Handle>;
}

impl<T: AsHandle + AsRawFd> ReadyExt for T {
    #[inline]
    fn ready(&self, poll: Poll) -> ReadyFuture<Self::Handle> {
        let fd = self.as_raw_fd();
        let handle = self.as_handle();

        ReadyFuture::new(fd, poll, handle)
    }
}

pub struct ReadyFuture<H: Handle>(H::Wait);

impl<H: Handle> ReadyFuture<H> {
    pub fn new(fd: RawFd, poll: Poll, handle: &H) -> ReadyFuture<H> {
        let entry = opcode::PollAdd::new(types::Target::Fd(fd), poll.bits())
            .build();

        ReadyFuture(unsafe { handle.push(entry) })
    }
}

impl<H> Future for ReadyFuture<H>
where
    H: Handle,
    H::Wait: Unpin
{
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> TaskPoll<Self::Output> {
        let cqe = ready!(Pin::new(&mut self.0).poll(cx))?;
        let ret = cqe.result();

        TaskPoll::Ready(if ret >= 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        })
    }
}
