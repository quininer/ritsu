use std::env;
use std::fs::File;
use std::io::{ self, Write };
use bytes::BytesMut;
use anyhow::Context;
use ritsu::Proactor;
use ritsu::actions;


fn main() -> anyhow::Result<()> {
    let target = env::args()
        .nth(1)
        .context("need file")?;

    let mut proactor = Proactor::new()?;
    let handle = proactor.handle();

    proactor.block_on(async move {
        let mut fd = File::open(target)?;
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        let mut buf = BytesMut::with_capacity(512 << 10);

        loop {
            let (fd2, ret) = actions::fs::read_buf(&handle, fd, buf).await;
            fd = fd2;
            buf = ret?;

            if buf.is_empty() {
                break
            }

            stdout.write_all(&buf)?;

            buf.clear();
        }

        Ok(()) as std::io::Result<()>
    })??;

    Ok(())
}
