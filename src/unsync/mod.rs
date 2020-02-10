pub(crate) mod oneshot;

use std::io;
use crate::{
    Proactor, RawHandle,
    Handle as TaskHandle,
    Ticket,
    SubmissionEntry, CompletionEntry
};


#[derive(Clone)]
pub struct Handle {
    handle: RawHandle,
}

impl Proactor<Handle> {
    pub fn handle(&self) -> Handle {
        Handle { handle: self.as_raw_handle() }
    }
}

impl TaskHandle for Handle {
    type Ticket = oneshot::Sender<CompletionEntry>;
    type Wait = oneshot::Receiver<CompletionEntry>;

    unsafe fn push(&self, entry: SubmissionEntry) -> io::Result<Self::Wait> {
        let (tx, rx) = oneshot::channel();

        self.handle.push::<Self::Ticket>(entry.user_data(tx.into_raw() as _))?;

        Ok(rx)
    }
}
