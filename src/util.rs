use std::io;
use bytes::{ Buf, BufMut, buf::IoSliceMut };


pub fn iovecs<B: Buf>(buf: &B) -> Vec<libc::iovec> {
    unsafe {
        let mut bufs: Vec<io::IoSlice> = Vec::with_capacity(32);
        bufs.set_len(bufs.capacity());

        let n = buf.bytes_vectored(&mut bufs);
        bufs.set_len(n);

        let (ptr, len, cap) = bufs.into_raw_parts();
        Vec::from_raw_parts(ptr as *mut _, len, cap)
    }
}


pub fn iovecs_mut<B: BufMut>(buf: &mut B) -> Vec<libc::iovec> {
    unsafe {
        let mut bufs: Vec<IoSliceMut> = Vec::with_capacity(32);
        bufs.set_len(bufs.capacity());

        let n = buf.bytes_vectored_mut(&mut bufs);
        bufs.set_len(n);

        let (ptr, len, cap) = bufs.into_raw_parts();
        Vec::from_raw_parts(ptr as *mut _, len, cap)
    }
}
