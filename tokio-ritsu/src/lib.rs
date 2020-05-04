// mod oneshot;

use std::{ io, mem };
use std::pin::Pin;
use std::task::{ Context, Poll };
use std::future::Future;
use tokio::sync::mpsc;
use ritsu::{
    Handle as TaskHandle, Ticket,
    SubmissionEntry
};


#[derive(Clone)]
pub struct Handle(mpsc::UnboundedSender<SubmissionEntry>);

pub struct Driver(mpsc::UnboundedReceiver<SubmissionEntry>);

impl Handle {
    pub fn new() -> (Driver, Handle) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Driver(rx), Handle(tx))
    }
}

impl Driver {
    pub async fn register(mut self, handle: TaskHandle) -> io::Result<()> {
        while let Some(sqe) = self.0.recv().await {
            unsafe {
                handle.push(sqe)?;
            }
        }

        Ok(())
    }
}
