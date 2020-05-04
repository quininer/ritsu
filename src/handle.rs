use std::{ io, mem };
use std::cell::RefCell;
use crate::sync::{ Ticket, TicketFuture };
use crate::action::{ Handle, HandleVTable };
use crate::{ RawHandle, SubmissionEntry };


thread_local!{
    static HANDLE: RefCell<Option<Handle>> = RefCell::new(None);
}

pub unsafe fn set(handle: Handle) {
    HANDLE.with(|h| {
        h.borrow_mut().replace(handle);
    });
}

pub unsafe fn push(entry: SubmissionEntry) -> io::Result<TicketFuture> {
    HANDLE.with(|h| Some(h.borrow().as_ref()?.push(entry)))
        .expect("not found ritsu runtime")
}


pub fn default_handle(raw_handle: RawHandle) -> Handle {
    static VTABLE: HandleVTable = HandleVTable {
        push, clone, drop
    };

    unsafe fn push(ptr: *const (), entry: SubmissionEntry) -> io::Result<TicketFuture> {
        let handle = RawHandle::from_raw(ptr as *const _);

        let (ticket, fut) = Ticket::new();

        handle.raw_push(ticket.register(entry))?;

        mem::forget(handle);
        Ok(fut)
    }

    unsafe fn clone(ptr: *const ()) -> Handle {
        let handle = RawHandle::from_raw(ptr as *const _);
        let handle2 = handle.clone();
        mem::forget(handle);

        default_handle(handle2)
    }

    unsafe fn drop(ptr: *const ()) {
        RawHandle::from_raw(ptr as *const _);
    }

    unsafe {
        Handle::new(raw_handle.into_raw() as *const (), &VTABLE)
    }
}
