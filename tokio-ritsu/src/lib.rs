#![feature(type_alias_impl_trait)]

mod oneshot;

use std::io;
use std::future::Future;
use tokio::sync::mpsc;
use ritsu::{
    Handle as TaskHandle, Ticket,
    RawHandle, SubmissionEntry
};


#[derive(Clone)]
pub struct Handle {
    sender: mpsc::UnboundedSender<SubmissionEntry>
}

type Driver = impl Future<Output = io::Result<()>>;

impl Handle {
    pub fn from(handle: RawHandle) -> (Driver, Handle) {
        let (tx, mut rx) = mpsc::unbounded_channel();

        let fut = async move {
            while let Some(sqe) = rx.recv().await {
                unsafe {
                    handle.push::<oneshot::Sender>(sqe)?;
                }
            }

            Ok(())
        };

        (fut, Handle { sender: tx })
    }
}

impl TaskHandle for Handle {
    type Ticket = oneshot::Sender;
    type Wait = oneshot::Receiver;

    unsafe fn push(&self, entry: SubmissionEntry) -> io::Result<Self::Wait> {
        let (tx, rx) = oneshot::channel();

        // TODO cloesd ?
        self.sender.send(entry.user_data(tx.into_raw() as _))
            .ok()
            .unwrap();

        Ok(rx)
    }
}
