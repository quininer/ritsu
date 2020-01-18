use std::{ fs, io, mem };
use std::rc::Rc;
use std::pin::Pin;
use std::cell::RefCell;
use std::future::Future;
use std::marker::PhantomData;
use std::task::{ Context, Poll };
use std::os::unix::io::AsRawFd;
use slab::Slab;
use io_uring::opcode::{ self, types };
use crate::{ oneshot, CompletionEntry, LocalHandle };


pub struct File {
    fd: fs::File,
    offset: i64,
    handle: LocalHandle,
}

thread_local!{
    static BUF_ONE: RefCell<Slab<libc::iovec>> = RefCell::new(Slab::new());
}

pub enum ReadFileFuture<'a> {
    Running {
        fd: &'a File,
        bufkey: BufKey,
        buf: Vec<u8>,
        rx: oneshot::Receiver<CompletionEntry>
    },
    Error(io::Error),
    End
}

impl File {
    pub fn from_std(fd: fs::File, handle: LocalHandle) -> File {
        File { fd, handle, offset: 0 }
    }

    pub fn read(&self, mut buf: Vec<u8>) -> ReadFileFuture<'_> {
        let (tx, rx) = oneshot::channel();

        let bufptr = unsafe { mem::transmute::<_, libc::iovec>(io::IoSliceMut::new(&mut buf)) };
        let (key, entry) = BUF_ONE.with(|bufs| {
            let mut bufs = bufs.borrow_mut();
            let entry = bufs.vacant_entry();
            let key = entry.key();
            let bufptr = entry.insert(bufptr);

            let op = types::Target::Fd(self.fd.as_raw_fd());
            (BufKey(key, PhantomData), opcode::Readv::new(op, bufptr, 1).offset(self.offset))
        });

        match unsafe { self.handle.push(tx, entry.build()) } {
            Ok(_) => ReadFileFuture::Running { fd: self, bufkey: key, buf, rx },
            Err(err) => ReadFileFuture::Error(err)
        }
    }
}

impl<'a> Future for ReadFileFuture<'a> {
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
