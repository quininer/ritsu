use std::io;
use std::pin::Pin;
use std::task::{ Context, Poll as TaskPoll };
use std::future::Future;
use std::os::unix::io::{ AsRawFd, RawFd };
use bitflags::bitflags;
use io_uring::opcode::{ self, types };
use crate::handle;
use crate::TicketFuture;


bitflags!{
    pub struct Poll: i16 {
        const READABLE = libc::POLLIN;
        const WRITABLE = libc::POLLOUT;

        // TODO
    }
}

pub trait ReadyExt: AsRawFd {
    fn ready(&self, poll: Poll) -> ReadyFuture;
}

impl<T: AsRawFd> ReadyExt for T {
    #[inline]
    fn ready(&self, poll: Poll) -> ReadyFuture {
        let fd = self.as_raw_fd();

        ReadyFuture::new(fd, poll)
    }
}

pub struct ReadyFuture(Option<io::Result<TicketFuture>>);

impl ReadyFuture {
    pub fn new(fd: RawFd, poll: Poll) -> ReadyFuture {
        let entry = opcode::PollAdd::new(types::Target::Fd(fd), poll.bits())
            .build();

        ReadyFuture(Some(unsafe { handle::push(entry) }))
    }
}

impl Future for ReadyFuture {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> TaskPoll<Self::Output> {
        let mut fut = match self.0.take() {
            Some(Ok(fut)) => fut,
            Some(Err(err)) => return TaskPoll::Ready(Err(err)),
            None => return TaskPoll::Pending
        };

        let ret = match Pin::new(&mut fut).poll(cx) {
            TaskPoll::Ready(cqe) => cqe.result(),
            TaskPoll::Pending => {
                self.0 = Some(Ok(fut));
                return TaskPoll::Pending
            }
        };

        TaskPoll::Ready(if ret >= 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        })
    }
}
