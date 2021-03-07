use std::{ io, fs };
use std::rc::Rc;
use std::time::Instant;
use std::cell::RefCell;
use bytes::BytesMut;
use anyhow::Context;
use tokio::task::{ LocalSet, yield_now };
use ritsu::{ actions, Proactor };
use crate::Options;
use crate::util::{ RcFile, plan };


pub(crate) fn main(options: &Options) -> anyhow::Result<()> {
    let mut proactor = Proactor::new()?;
    let handle = proactor.handle();
    let taskset = LocalSet::new();

    let fd = fs::File::open(&options.target)?;
    let total = fd.metadata()?.len();

    let bufsize = options.bufsize;
    let queue = plan(total, &options);
    let bufpool = (0..128)
        .map(|_| BytesMut::with_capacity(bufsize))
        .collect::<Vec<_>>();
    let bufpool = Rc::new(RefCell::new(bufpool));

    ritsu::block_on(&mut proactor, async move {
        let fd = RcFile(Rc::new(fd));
        let now = Instant::now();

        for start in queue {
            let handle = handle.clone();
            let fd = fd.clone();
            let bufpool = bufpool.clone();

            taskset.spawn_local(async move {
                let buf = loop {
                    let buf = bufpool.borrow_mut().pop();
                    if let Some(buf) = buf {
                        break buf;
                    } else {
                        yield_now().await;
                        break bufpool.borrow_mut()
                            .pop()
                            .unwrap_or_else(|| BytesMut::with_capacity(bufsize));
                    }
                };

                let (_, buf) = actions::io::read_buf(&handle, &mut Some(fd), buf, Some(start as _))
                    .await?;

                bufpool.borrow_mut().push(buf);

                Ok(()) as io::Result<()>
            });
        }

        taskset.await;

        println!("dur: {:?}", now.elapsed());

        Ok(()) as io::Result<()>
    })??;

    Ok(())
}
