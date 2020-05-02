#![allow(dead_code)]

pub use std::sync;

pub mod cell {
    pub struct UnsafeCell<T>(std::cell::UnsafeCell<T>);

    impl<T> UnsafeCell<T> {
        #[inline]
        pub fn new(t: T) -> UnsafeCell<T> {
            UnsafeCell(std::cell::UnsafeCell::new(t))
        }

        #[inline]
        pub fn with<F, R>(&self, f: F) -> R
        where F: FnOnce(*const T) -> R
        {
            f(self.0.get())
        }

        #[inline]
        pub fn with_mut<F, R>(&self, f: F) -> R
        where F: FnOnce(*mut T) -> R
        {
            f(self.0.get())
        }
    }
}
