use std::fs::File;
use bytes::BytesMut;
use ritsu::Proactor;
use ritsu::actions;


fn main() -> anyhow::Result<()> {
    let mut proactor = Proactor::new()?;
    let handle = proactor.handle();

    let fd = File::open("Cargo.toml")?;

    proactor.block_on(async move {
        let buf = BytesMut::with_capacity(64);

        let (_fd, ret) = actions::fs::read_buf(&handle, fd, buf).await;
        let buf = ret?;

        println!("{:?}", String::from_utf8_lossy(&buf));

        Ok(()) as std::io::Result<()>
    })??;

    Ok(())
}
