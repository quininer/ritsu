use std::fs;
use io_uring::{ squeue, cqueue, IoUring };
use crate::{ Handle, Action };


pub struct File {
    fd: fs::File,
    handle: Handle
}

pub struct ReadFile<T> {
    fd: File,
    buf: T
}

pub fn read<T: AsMut<[u8]>>(fd: File, buf: T) -> ReadFile<T> {
    ReadFile { fd, buf }
}

impl Action for ReadFile {
    unsafe fn build_request(&self) -> squeue::Entry {
        todo!()
    }
}
