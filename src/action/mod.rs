use io_uring::{ squeue, cqueue };
use crate::channel::Channel;


pub type SubmissionEntry = squeue::Entry;
pub type CompletionEntry = cqueue::Entry;
