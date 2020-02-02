use std::{ fs, io, mem };
use std::rc::Rc;
use std::pin::Pin;
use std::cell::RefCell;
use std::future::Future;
use std::marker::{ PhantomData, Unpin };
use std::task::{ Context, Poll };
use std::os::unix::io::AsRawFd;
use slab::Slab;
use io_uring::opcode::{ self, types };
use crate::{ oneshot, Ticket, CompletionEntry, Handle };


pub struct File<H: Handle> {
    fd: fs::File,
    offset: i64,
    handle: H,
}

thread_local!{
    static BUF_ONE: RefCell<Slab<libc::iovec>> = RefCell::new(Slab::new());
}

pub enum ReadFileFuture<'a, H: Handle> {
    Running {
        fd: &'a File<H>,
        bufkey: BufKey,
        buf: Vec<u8>,
        rx: H::Wait
    },
    Error(io::Error),
    End
}

impl<H: Handle> File<H> {
    pub fn from_std(fd: fs::File, handle: H) -> File<H> {
        File { fd, handle, offset: 0 }
    }

    pub fn read(&self, mut buf: Vec<u8>) -> ReadFileFuture<'_, H> {
        let bufptr = unsafe { mem::transmute::<_, libc::iovec>(io::IoSliceMut::new(&mut buf)) };
        let (key, entry) = BUF_ONE.with(|bufs| {
            let mut bufs = bufs.borrow_mut();
            let entry = bufs.vacant_entry();
            let key = entry.key();
            let bufptr = entry.insert(bufptr);

            let op = types::Target::Fd(self.fd.as_raw_fd());
            (BufKey(key, PhantomData), opcode::Readv::new(op, bufptr, 1).offset(self.offset))
        });

        match unsafe { self.handle.push(entry.build()) } {
            Ok(rx) => ReadFileFuture::Running { fd: self, bufkey: key, buf, rx },
            Err(err) => ReadFileFuture::Error(err)
        }
    }
}

impl<'a, H> Future for ReadFileFuture<'a, H>
where
    H: Handle,
    H::Wait: Unpin
{
    type Output = io::Result<Vec<u8>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match mem::replace(this, ReadFileFuture::End) {
            ReadFileFuture::Running { fd, bufkey, buf, mut rx } => {
                match Pin::new(&mut rx).poll(cx) {
                    Poll::Ready(cqe) => {
                        let res = cqe.result();
                        if res >= 0 {
                            Poll::Ready(Ok(buf))
                        } else {
                            Poll::Ready(Err(io::Error::from_raw_os_error(-res)))
                        }
                    },
                    Poll::Pending => {
                        *this = ReadFileFuture::Running { fd, bufkey, buf, rx };
                        Poll::Pending
                    }
                }
            }
            ReadFileFuture::Error(err) => Poll::Ready(Err(err)),
            ReadFileFuture::End => panic!()
        }
    }
}

pub struct BufKey(usize, PhantomData<Rc<()>>);

impl Drop for BufKey {
    fn drop(&mut self) {
        let _ = BUF_ONE.try_with(|bufs| bufs.borrow_mut().remove(self.0));
    }
}
