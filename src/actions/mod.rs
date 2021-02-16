pub mod io;
pub mod fs;
pub mod time;

use std::pin::Pin;
use std::future::Future;
use std::mem::MaybeUninit;
use std::task::{ Context, Poll };
use io_uring::{ squeue, cqueue };
use pin_project_lite::pin_project;
use crate::ticket::TicketFuture;
use crate::handle::Handle;


pin_project!{
    pub struct Action<T: 'static> {
        hold: MaybeUninit<T>,
        #[pin]
        ticket: TicketFuture
    }
}

pub unsafe fn action<T: 'static>(handle: &dyn Handle, value: T, entry: squeue::Entry)
    -> Action<T>
{
    let ticket = handle.push(entry);
    let hold = MaybeUninit::new(value);

    Action { hold, ticket }
}

impl<T: 'static> Future for Action<T> {
    type Output = (T, cqueue::Entry);

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.ticket.poll(cx) {
            Poll::Ready(entry) => {
                let val = unsafe { this.hold.as_ptr().read() };
                Poll::Ready((val, entry))
            },
            Poll::Pending => return Poll::Pending
        }
    }
}

pub fn cancel<T: 'static>(handle: &dyn Handle, action: Action<T>) {
    use io_uring::opcode;
    use crate::EMPTY_TOKEN;

    if action.ticket.is_closed() {
        return;
    }

    let cancel_e = opcode::AsyncCancel::new(action.ticket.as_ptr().as_ptr() as _)
        .build()
        .user_data(EMPTY_TOKEN);

    unsafe {
        handle.push(cancel_e);
    }
}
