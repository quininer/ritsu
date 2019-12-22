use io_uring::{ squeue, cqueue };


pub type SubmissionEntry = squeue::Entry;
pub type CompletionEntry = cqueue::Entry;
