//! This is a temporary solution because tokio 0.2 does not expose the Park interface.

use futures::future::LocalFutureObj;
use futures::stream::futures_unordered::FuturesUnordered;


pub struct LocalPool {
    queue: FuturesUnordered<LocalFutureObj<'static, ()>>,
    pending: Vec<LocalFutureObj<'static, ()>>
}

impl LocalPool {
    pub fn run(&mut self) {
        // TODO
        //
        // run task
        // reset waker
        // submit queue
        // wait queue
    }
}
