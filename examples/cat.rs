use std::{ io, env };
use std::path::Path;
use bytes::BytesMut;
use anyhow::Context;
use ritsu::{ actions, Proactor };


fn main() -> anyhow::Result<()> {
    let target = env::args()
        .nth(1)
        .context("need file")?;

    let mut proactor = Proactor::new()?;
    let handle = proactor.handle();

    ritsu::block_on(&mut proactor, async move {
        let target = Path::new(&target);
        let mut fd = actions::fs::open(&handle, target).await?;
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
