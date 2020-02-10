//! This is a temporary solution because tokio 0.2 does not expose the Park interface.
//!
//! fork from `futures-executor/local_pool.rs`.

use std::io;
use std::cell::RefCell;
use std::future::Future;
use std::rc::{ Rc, Weak };
use std::task::{ Context, Poll };
use futures_task::LocalFutureObj;
use futures_util::pin_mut;
use futures_util::stream::{ StreamExt, FuturesUnordered };
use crate::{ Proactor, Handle };
use crate::unsync;

/// A single-threaded task pool for polling futures to completion.
pub struct Runtime<H: Handle> {
    pool: FuturesUnordered<LocalFutureObj<'static, ()>>,
    incoming: Rc<Incoming>,
    proactor: Proactor<H>
}

#[derive(Clone, Debug)]
pub struct Spawner {
    incoming: Weak<Incoming>,
}

type Incoming = RefCell<Vec<LocalFutureObj<'static, ()>>>;

impl<H: Handle> Runtime<H> {
    /// Create a new, empty pool of tasks.
    pub fn new() -> io::Result<Runtime<H>> {
        Ok(Runtime {
            pool: FuturesUnordered::new(),
            incoming: Default::default(),
            proactor: Proactor::new()?
        })
    }

    /// Get a clonable handle to the pool as a `Spawn`.
    pub fn spawner(&self) -> Spawner {
        Spawner {
            incoming: Rc::downgrade(&self.incoming),
        }
    }

    /// Run all tasks in the pool to completion.
    ///
    /// ```
    /// use ritsu::executor::Runtime;
    /// use ritsu::unsync;
    ///
    /// let mut pool: Runtime<unsync::Handle> = Runtime::new().unwrap();
    ///
    /// // ... spawn some initial tasks using `spawn.spawn()` or `spawn.spawn_local()`
    ///
    /// // run *all* tasks in the pool to completion, including any newly-spawned ones.
    /// pool.run();
    /// ```
    ///
    /// The function will block the calling thread until *all* tasks in the pool
    /// are complete, including any spawned while running existing tasks.
    pub fn run(&mut self) {
        let Runtime { pool, incoming, proactor } = self;
        run_executor(proactor, |cx| poll_pool(pool, incoming, cx))
    }

    /// Runs all the tasks in the pool until the given future completes.
    ///
    /// ```
    /// use ritsu::executor::Runtime;
    /// use ritsu::unsycn;
    ///
    /// let mut pool: Runtime<unsync::Handle> = Runtime::new().unwrap();
    /// # let my_app  = async {};
    ///
    /// // run tasks in the pool until `my_app` completes
    /// pool.run_until(my_app);
    /// ```
    ///
    /// The function will block the calling thread *only* until the future `f`
    /// completes; there may still be incomplete tasks in the pool, which will
    /// be inert after the call completes, but can continue with further use of
    /// one of the pool's run or poll methods. While the function is running,
    /// however, all tasks in the pool will try to make progress.
    pub fn run_until<F: Future>(&mut self, future: F) -> F::Output {
        let Runtime { pool, incoming, proactor } = self;

        pin_mut!(future);

        run_executor(proactor, |cx| {
            {
                // if our main task is done, so are we
                let result = future.as_mut().poll(cx);
                if let Poll::Ready(output) = result {
                    return Poll::Ready(output);
                }
            }

            let _ = poll_pool(pool, incoming, cx);
            Poll::Pending
        })
    }
}

impl Runtime<unsync::Handle> {
    pub fn handle(&self) -> unsync::Handle {
        self.proactor.handle()
    }
}

impl Spawner {
    pub fn spawn<F: Future<Output = ()> + 'static>(&self, fut: F) {
        if let Some(incoming) = self.incoming.upgrade() {
            incoming.borrow_mut().push(LocalFutureObj::from(Box::pin(fut)));
        }
    }
}

// Set up and run a basic single-threaded spawner loop, invoking `f` on each
// turn.
fn run_executor<T, H: Handle>(
    proactor: &mut Proactor<H>,
    mut f: impl FnMut(&mut Context<'_>) -> Poll<T>
) -> T {
    loop {
        let waker = proactor.waker_ref();
        let mut cx = Context::from_waker(&waker);

        if let Poll::Ready(t) = f(&mut cx) {
            return t;
        }

        proactor.park(None).expect("Proactor park failed");
    }
}

// Make maximal progress on the entire pool of spawned task, returning `Ready`
// if the pool is empty and `Pending` if no further progress can be made.
fn poll_pool(
    pool: &mut FuturesUnordered<LocalFutureObj<'static, ()>>,
    incoming: &Rc<Incoming>,
    cx: &mut Context<'_>
) -> Poll<()> {
    // state for the FuturesUnordered, which will never be used
    loop {
        let ret = {
            // empty the incoming queue of newly-spawned tasks
            {
                let mut incoming = incoming.borrow_mut();
                for task in incoming.drain(..) {
                    pool.push(task)
                }
            }

            // try to execute the next ready future
            pool.poll_next_unpin(cx)
        };

        // we queued up some new tasks; add them and poll again
        if !incoming.borrow().is_empty() {
            continue;
        }

        // no queued tasks; we may be done
        match ret {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(None) => return Poll::Ready(()),
            _ => {}
        }
    }
}
