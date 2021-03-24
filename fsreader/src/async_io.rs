use std::{ io, fs };
use std::rc::Rc;
use std::time::Instant;
use std::cell::RefCell;
use std::os::unix::fs::OpenOptionsExt;
use tokio::task::{ LocalSet, yield_now };
use ritsu::{ actions, Proactor };
use crate::Options;
use crate::util::{ RcFile, plan, AlignedBuffer };


pub(crate) fn main(options: &Options) -> anyhow::Result<()> {
    let mut proactor = Proactor::new()?;
    let handle = proactor.handle();

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
    let bufpool = (0..128)
        .map(|_| AlignedBuffer::with_capacity(bufsize))
        .collect::<Vec<_>>();
    let bufpool = Rc::new(RefCell::new(bufpool));

    ritsu::block_on(&mut proactor, async move {
        let fd = RcFile(Rc::new(fd));

        for _ in 0..count {
            let taskset = LocalSet::new();
            let mut jobs = Vec::with_capacity(queue.len());
            let now = Instant::now();

            for &start in queue.iter() {
                let handle = handle.clone();
                let fd = fd.clone();
                let bufpool = bufpool.clone();

                let j = taskset.spawn_local(async move {
                    let buf = {
                        let buf = bufpool.borrow_mut().pop();
                        if let Some(buf) = buf {
                            buf
                        } else {
                            yield_now().await;
                            bufpool.borrow_mut()
                                .pop()
                                .unwrap_or_else(|| AlignedBuffer::with_capacity(bufsize))
                        }
                    };

                    let (_, mut buf) = actions::io::read_buf(&handle, &mut Some(fd), buf, Some(start as _))
                        .await?;

                    buf.clear();

                    bufpool.borrow_mut().push(buf);

                    Ok(()) as io::Result<()>
                });

                jobs.push(j);
            }

            taskset.await;

            for j in jobs {
                j.await??;
            }

            println!("dur: {:?}", now.elapsed());
        }

        Ok(()) as io::Result<()>
    })??;

    Ok(())
}
