use std::{ io, env };
use std::fs::File;
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
        let mut stdout = io::stdout();
        let mut buf = BytesMut::with_capacity(32 << 10);

        loop {
            let (fd2, buf2) =
                actions::io::read_buf(&handle, &mut Some(fd), buf, None).await?;
            fd = fd2;
            buf = buf2;

            if buf.is_empty() {
                break
            }

            let (stdout2, buf2) =
                actions::io::write_buf(&handle, &mut Some(stdout), buf, None).await?;
            stdout = stdout2;
            buf = buf2;

            buf.clear();
        }

        Ok(()) as std::io::Result<()>
    })??;

    Ok(())
}
