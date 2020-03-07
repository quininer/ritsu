use std::{ io, mem };
use bytes::{ Buf, BufMut, Bytes, BytesMut, buf::IoSliceMut };


#[repr(transparent)]
pub struct IoVec(libc::iovec);

unsafe impl Send for IoVec {}
unsafe impl Sync for IoVec {}

impl IoVec {
    #[inline]
    pub const fn as_ptr(self: &'_ Box<IoVec>) -> *const libc::iovec {
        &self.0
    }
}

pub fn boxed_iovec_mut(buf: &mut BytesMut) -> Box<IoVec> {
    let buf = IoSliceMut::from(buf.bytes_mut());
    Box::new(unsafe { mem::transmute(buf) })
}

pub fn iovecs2(bufs: &[Bytes]) -> Vec<IoVec> {
    let bufs = bufs.iter()
        .map(|bytes| io::IoSlice::new(bytes.bytes()))
        .collect::<Vec<_>>();

    unsafe {
        let (ptr, len, cap) = bufs.into_raw_parts();
        Vec::from_raw_parts(ptr as *mut _, len, cap)
    }
}

pub fn iovecs<B: Buf>(buf: &B) -> Vec<IoVec> {
    unsafe {
        let mut bufs: Vec<io::IoSlice> = Vec::with_capacity(32);
        bufs.set_len(bufs.capacity());

        let n = buf.bytes_vectored(&mut bufs);
        bufs.set_len(n);

        let (ptr, len, cap) = bufs.into_raw_parts();
        Vec::from_raw_parts(ptr as *mut _, len, cap)
    }
}


pub fn iovecs_mut<B: BufMut>(buf: &mut B) -> Vec<IoVec> {
    unsafe {
        let mut bufs: Vec<IoSliceMut> = Vec::with_capacity(32);
        bufs.set_len(bufs.capacity());

        let n = buf.bytes_vectored_mut(&mut bufs);
        bufs.set_len(n);

        let (ptr, len, cap) = bufs.into_raw_parts();
        Vec::from_raw_parts(ptr as *mut _, len, cap)
    }
}
