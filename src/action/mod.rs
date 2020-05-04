// pub mod iohelp;
pub mod fs;
pub mod timeout;
pub mod tcp;
pub mod poll;

use crate::Handle;


pub trait AsHandle {
    fn as_handle(&self) -> &Handle;
}
