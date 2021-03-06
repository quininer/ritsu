pub mod io;
pub mod fs;
pub mod time;

use std::pin::Pin;
use std::future::Future;
use std::mem::MaybeUninit;
use std::task::{ Context, Poll };
use io_uring::{ squeue, cqueue };
use pin_project_lite::pin_project;
use crate::ticket::{ Ticket, TicketFuture };
use crate::handle::Handle;


pin_project!{
    pub struct Action<T: 'static> {
        hold: MaybeUninit<T>,
        #[pin]
        ticket: TicketFuture
    }
}

pub struct PushError<T> {
    error: std::io::Error,
    value: T
}

pub unsafe fn action<H: Handle, T: 'static>(handle: H, value: T, entry: squeue::Entry)
    -> Result<Action<T>, PushError<T>>
{
    let (tx, ticket) = Ticket::new();
    let tx_ptr = tx.into_raw();
    let entry = entry.user_data(tx_ptr.as_ptr() as _);

    match handle.push(&entry) {
        Ok(()) => {
            let hold = MaybeUninit::new(value);
            Ok(Action { hold, ticket })
        },
        Err(error) => {
            Ticket::from_raw(tx_ptr);
            Err(PushError { error, value })
        }
    }
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
            Poll::Pending => Poll::Pending
        }
    }
}

impl<T> PushError<T> {
    #[inline]
    pub fn into_inner(self) -> (std::io::Error, T) {
        (self.error, self.value)
    }

    #[inline]
    pub fn into_error(self) -> std::io::Error {
        self.error
    }
}

pub fn cancel<H: Handle, T: 'static>(handle: H, action: Action<T>) -> std::io::Result<()> {
    use io_uring::opcode;
    use crate::EMPTY_TOKEN;

    if action.ticket.is_closed() {
        return Ok(());
    }

    let cancel_e = opcode::AsyncCancel::new(action.ticket.as_ptr().as_ptr() as _)
        .build()
        .user_data(EMPTY_TOKEN);

    unsafe {
        handle.push(&cancel_e)?;
    }

    Ok(())
}

pub async fn nop<H: Handle>(handle: H) -> std::io::Result<()> {
    use io_uring::opcode;

    let nop_e = opcode::Nop::new().build();

    unsafe {
        action(handle, (), nop_e)
            .map_err(PushError::into_error)?.await;
    }

    Ok(())
}
