use std::{ io, fs };
use std::rc::Rc;
use std::time::Instant;
use std::cell::RefCell;
use std::os::unix::io::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use bytes::BufMut;
use tokio::task::{ LocalSet, yield_now };
use crate::Options;
use crate::util::{ RcFile, plan, AlignedBuffer };


pub(crate) fn main(options: &Options) -> anyhow::Result<()> {
    let mut open_options = fs::OpenOptions::new();

    if options.direct {
        open_options.custom_flags(libc::O_DIRECT);
    }

    let fd = open_options
        .read(true)
        .open(&options.target)?;
    let total = fd.metadata()?.len();

    let bufsize = options.bufsize;
    let queue = plan(total, &options);
    let mut buf = AlignedBuffer::with_capacity(bufsize);

    let now = Instant::now();

    for start in queue {
        unsafe {
            let chunk = buf.chunk_mut();

            let ret = libc::pread(
                fd.as_raw_fd(),
                chunk.as_mut_ptr().cast(),
                chunk.len(),
                start as _
            );

            if ret == -1 {
                return Err(io::Error::last_os_error().into());
            }

            buf.advance_mut(ret as _);
        }

        buf.clear();
    }

    println!("dur: {:?}", now.elapsed());

    Ok(())
}
