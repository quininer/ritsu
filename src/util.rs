use std::{ io, mem };
use std::ops::{ Deref, DerefMut };
use std::mem::MaybeUninit;
use bytes::{ Buf, BufMut };


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

pub fn ioret(ret: i32) -> io::Result<i32> {
    if ret >= 0 {
        Ok(ret)
    } else {
        Err(io::Error::from_raw_os_error(-ret))
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
        &self.inner
    }
}

impl<T> DerefMut for MaybeLock<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
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

pub struct Buffer {
    buf: Box<[MaybeUninit<u8>]>,
    wlen: usize,
    rlen: usize
}

impl Buffer {
    pub fn new(n: usize) -> Buffer {
        Buffer {
            buf: Box::new_uninit_slice(n),
            wlen: 0,
            rlen: 0
        }
    }

    pub fn clear(&mut self) {
        self.wlen = 0;
        self.rlen = 0;
    }
}

impl Buf for Buffer {
    fn remaining(&self) -> usize {
        self.wlen - self.rlen
    }

    fn bytes(&self) -> &[u8] {
        unsafe {
            MaybeUninit::slice_get_ref(&self.buf[self.rlen..self.wlen])
        }
    }

    fn advance(&mut self, n: usize) {
        assert!(self.rlen + n <= self.wlen);

        self.rlen += n;
    }
}

impl BufMut for Buffer {
    fn remaining_mut(&self) -> usize {
        self.buf.len() - self.wlen
    }

    unsafe fn advance_mut(&mut self, n: usize) {
        assert!(self.wlen + n <= self.buf.len());

        self.wlen += n;
    }

    fn bytes_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        &mut self.buf[self.wlen..]
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
