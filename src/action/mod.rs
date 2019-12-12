use io_uring::{ squeue, cqueue };
use crate::channel::Channel;


pub type SubmissionEntry = squeue::Entry;
pub type CompletionEntry = cqueue::Entry;


pub trait Action<C: Channel<CompletionEntry>> {
    unsafe fn build_request(&self)
        -> (C::Sender, SubmissionEntry);
}
