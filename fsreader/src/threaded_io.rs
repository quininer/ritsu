use std::{ io, fs };
use std::time::Instant;
use std::sync::atomic::{ self, AtomicU64 };
use std::os::unix::io::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use rayon::ThreadPoolBuilder;
use bytes::BufMut;
use crate::Options;
use crate::util::{ plan, AlignedBuffer };


pub fn main(options: &Options) -> anyhow::Result<()> {
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

    ThreadPoolBuilder::new().build_global()?;

    for _ in 0..count {
        let mut size_count = AtomicU64::new(0);
        let now = Instant::now();

        rayon::scope(|scope| {
            let fd = &fd;
            let size_count = &size_count;

            for &start in queue.iter() {
                scope.spawn(move |_| {
                    let mut buf = AlignedBuffer::with_capacity(bufsize);

                    unsafe {
                        let chunk = buf.chunk_mut();

                        let ret = libc::pread(
                            fd.as_raw_fd(),
                            chunk.as_mut_ptr().cast(),
                            chunk.len(),
                            start as _
                        );

                        if ret == -1 {
                            panic!("{}", io::Error::last_os_error());
                        }

                        buf.advance_mut(ret as _);
                    }

                    size_count.fetch_add(buf.len() as u64, atomic::Ordering::Relaxed);

                    buf.clear();
                });
            }

            Ok(()) as anyhow::Result<()>
        })?;

        println!("dur: {:?}", now.elapsed());

        let size_count = *size_count.get_mut();
        let total = total * options.repeat as u64;
        if total != size_count as u64 {
            println!("expected: {}, actual: {}", total, size_count);
        }
    }

    Ok(())
}
