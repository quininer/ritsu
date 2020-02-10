use std::{ io, mem };
use std::pin::Pin;
use std::marker::Unpin;
use std::task::{ Context, Poll as TaskPoll };
use std::future::Future;
use std::os::unix::io::{ AsRawFd, RawFd };
use bitflags::bitflags;
use io_uring::opcode::{ self, types };
use crate::action::AsHandle;
use crate::Handle;


bitflags!{
    pub struct Poll: i16 {
        const IN = libc::POLLIN;
        const OUT = libc::POLLOUT;

        // TODO
    }
}

pub trait ReadyExt: AsHandle + AsRawFd {
    fn ready(&self, poll: Poll) -> ReadyFuture<Self::Handle> {
        let fd = self.as_raw_fd();
        let handle = self.as_handle();

        ReadyFuture::new(fd, poll, handle)
    }
}

impl<T: AsHandle + AsRawFd> ReadyExt for T {}

pub struct ReadyFuture<H: Handle>(Inner<H>);

enum Inner<H: Handle> {
    Fut(H::Wait),
    Err(io::Error),
    End
}

impl<H: Handle> ReadyFuture<H> {
    pub fn new(fd: RawFd, poll: Poll, handle: &H) -> ReadyFuture<H> {
        let entry = opcode::PollAdd::new(types::Target::Fd(fd), poll.bits())
            .build();

        match unsafe { handle.push(entry) } {
            Ok(fut) => ReadyFuture(Inner::Fut(fut)),
            Err(err) => ReadyFuture(Inner::Err(err))
        }
    }
}

impl<H> Future for ReadyFuture<H>
where
    H: Handle,
    H::Wait: Unpin
{
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> TaskPoll<Self::Output> {
        match mem::replace(&mut self.0, Inner::End) {
            Inner::Fut(mut fut) => match Pin::new(&mut fut).poll(cx) {
                TaskPoll::Ready(cqe) => {
                    let ret = cqe.result();
                    if ret >= 0 {
                        TaskPoll::Ready(Ok(()))
                    } else {
                        TaskPoll::Ready(Err(io::Error::from_raw_os_error(-ret)))
                    }
                },
                TaskPoll::Pending => {
                    self.0 = Inner::Fut(fut);
                    TaskPoll::Pending
                }
            },
            Inner::Err(err) => TaskPoll::Ready(Err(err)),
            Inner::End => panic!()
        }
    }
}
