use std::future::Future;
use futures::future::FusedFuture;


pub trait Channel<T> {
    type Sender: Sender<T>;
    type Receiver: Receiver<T>;

    fn new() -> (Self::Sender, Self::Receiver);
}

pub trait Sender<T> {
    fn into_raw(self) -> *const ();
    unsafe fn from_raw(ptr: *const ()) -> Self;

    fn send(self, item: T) -> Result<(), T>;
}

pub trait Receiver<T>:
    Future<Output = T> +
    FusedFuture
{}
