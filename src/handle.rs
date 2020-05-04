use std::{ io, mem };
use std::cell::RefCell;
use io_uring::squeue;
use crate::sync::{ Ticket, TicketFuture };
use crate::action::{ Handle, HandleVTable };
use crate::RawHandle;


thread_local!{
    static HANDLE: RefCell<Option<Handle>> = RefCell::new(None);
}

pub unsafe fn set(handle: Handle) {
    HANDLE.with(|h| {
        h.borrow_mut().replace(handle);
    });
}

pub unsafe fn push(entry: squeue::Entry) -> io::Result<TicketFuture> {
    HANDLE.with(|h| Some(h.borrow().as_ref()?.push(entry)))
        .expect("not found ritsu runtime")
}


pub fn default_handle(raw_handle: RawHandle) -> Handle {
    static VTABLE: HandleVTable = HandleVTable {
        push, clone, drop
    };

    unsafe fn push(ptr: *const (), entry: squeue::Entry) -> io::Result<TicketFuture> {
        let handle = RawHandle::from_raw(ptr as *const _);

        let (ticket, fut) = Ticket::new();
        let ptr = ticket.into_raw().as_ptr();

        handle.raw_push(entry.user_data(ptr as _))?;

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
