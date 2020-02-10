pub mod fs;
pub mod timeout;
pub mod tcp;
pub mod poll;

use crate::Handle;

pub trait AsHandle {
    type Handle: Handle;

    fn as_handle(&self) -> &Self::Handle;
}
