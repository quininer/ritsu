use std::mem;
use std::ops::{ Deref, DerefMut };


#[macro_export]
macro_rules! safety_await {
    ( ( $( $lock:expr ),* ) ; [ $( $res:ident ),* ] ; $push:expr ) => {{
        $(
            $lock.start();
        )*

        $(
            let res = std::mem::ManuallyDrop::new($res);
        )*

        let ret = match $push {
            Ok(fut) => Ok(fut.await),
            Err(err) => Err(err)
        };

        $(
            $res = std::mem::ManuallyDrop::into_inner(res);
        )*

        $(
            $lock.end();
        )*

        ret
    }};

    ( [ $( $res:ident ),* ] ; $push:expr ) => {
        safety_await!((); [ $( $res ),* ]; $push)
    };

    ( ( $( $lock:expr ),* ) ; $push:expr ) => {
        safety_await!(( $( $lock ),* ); []; $push)
    };

    ( $push:expr ) => {
        safety_await!((); []; $push)
    }
}


pub struct MaybeLock<T> {
    lock: bool,
    inner: mem::ManuallyDrop<T>
}

impl<T> MaybeLock<T> {
    #[inline]
    pub fn new(inner: T) -> MaybeLock<T> {
        MaybeLock {
            lock: false,
            inner: mem::ManuallyDrop::new(inner)
        }
    }

    #[inline]
    pub fn is_locked(&self) -> bool {
        self.lock
    }

    #[inline]
    fn asssert(&self) {
        assert!(!self.lock, "This resource is locked.");
    }

    #[inline]
    pub fn start(&mut self) {
        self.lock = true;
    }

    #[inline]
    pub fn end(&mut self) {
        self.lock = false;
    }
}

impl<T> Deref for MaybeLock<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.asssert();

        &self.inner
    }
}

impl<T> DerefMut for MaybeLock<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.asssert();

        &mut self.inner
    }
}

impl<T> Drop for MaybeLock<T> {
    fn drop(&mut self) {
        if !self.lock {
            unsafe {
                mem::ManuallyDrop::drop(&mut self.inner);
            }
        }
    }
}


#[test]
fn test_async_drop() {
    use std::task::Context;
    use std::future::Future;
    use futures_util::pin_mut;
    use futures_util::future;
    use futures_util::task::noop_waker_ref;

    struct Bad;

    impl Drop for Bad {
        fn drop(&mut self) {
            panic!()
        }
    }

    let fut = async {
        let mut bad = MaybeLock::new(Bad);

        bad.start();
        future::pending::<()>().await;
        bad.end();
    };

    {
        let fut = fut;
        pin_mut!(fut);

        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(fut.as_mut().poll(&mut cx).is_pending());
        assert!(fut.as_mut().poll(&mut cx).is_pending());
    }
}
