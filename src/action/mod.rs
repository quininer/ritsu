pub mod fs;
pub mod timeout;
pub mod tcp;
pub mod poll;

use std::io;
use crate::sync::TicketFuture;
use crate::SubmissionEntry;


pub struct Handle {
    ptr: *const (),
    vtable: &'static HandleVTable
}

pub struct HandleVTable {
    pub push: unsafe fn(*const (), SubmissionEntry) -> io::Result<TicketFuture>,
    pub clone: unsafe fn(*const ()) -> Handle,
    pub drop: unsafe fn(*const ())
}

impl Handle {
    pub const unsafe fn new(ptr: *const (), vtable: &'static HandleVTable) -> Handle {
        Handle { ptr, vtable }
    }

    #[inline]
    pub unsafe fn push(&self, entry: SubmissionEntry) -> io::Result<TicketFuture> {
        (self.vtable.push)(self.ptr, entry)
    }
}

impl Clone for Handle {
    #[inline]
    fn clone(&self) -> Handle {
        unsafe {
            (self.vtable.clone)(self.ptr)
        }
    }
}

impl Drop for Handle {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            (self.vtable.drop)(self.ptr)
        }
    }
}
