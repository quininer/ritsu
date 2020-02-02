macro_rules! alloc {
    (
        static $static_name:ident = $name:ident < $t:ty > as $keyname:ident $( ; )?
    ) => {
        thread_local!{
            static $static_name: $name = $name::new();
        }

        struct $name {
            slab: std::cell::RefCell<std::mem::ManuallyDrop<slab::Slab<$t>>>
        }

        impl $name {
            fn new() -> $name {
                $name {
                    slab: std::cell::RefCell::new(std::mem::ManuallyDrop::new(slab::Slab::new()))
                }
            }

            fn alloc<F, R>(&self, item: $t, f: F) -> ($keyname, R)
            where
                F: FnOnce(&mut $t) -> R
            {
                let mut slab = self.slab.borrow_mut();
                let entry = slab.vacant_entry();
                let key = $keyname(entry.key(), std::marker::PhantomData);
                let item = entry.insert(item);
                let ret = f(item);
                (key, ret)
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                let mut slab = self.slab.borrow_mut();
                if slab.is_empty() {
                    unsafe {
                        std::mem::ManuallyDrop::drop(&mut slab);
                    }
                } else {
                    slab.shrink_to_fit();
                }
            }
        }

        struct $keyname(usize, std::marker::PhantomData<std::rc::Rc<()>>);

        impl Drop for $keyname {
            fn drop(&mut self) {
                let _ = $static_name.try_with(|alloc| {
                    alloc.slab.borrow_mut().remove(self.0);
                });
            }
        }
    }
}
