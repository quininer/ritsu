use std::{ io, fs };
use std::time::Instant;
use std::os::unix::io::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use bytes::BufMut;
use crate::Options;
use crate::util::{ plan, AlignedBuffer };


pub(crate) fn main(options: &Options) -> anyhow::Result<()> {
    let mut open_options = fs::OpenOptions::new();

    if options.direct {
        open_options.custom_flags(libc::O_DIRECT);
    }

    let fd = open_options
        .read(true)
        .open(&options.target)?;
    let total = fd.metadata()?.len();

    let count = options.count;
    let bufsize = options.bufsize;
    let queue = plan(total, &options);
    let mut buf = AlignedBuffer::with_capacity(bufsize);

    for _ in 0..count {
        let now = Instant::now();

        for &start in queue.iter() {
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
    }

    Ok(())
}
