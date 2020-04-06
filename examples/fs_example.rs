use std::io;
use std::fs::File as StdFile;
use ritsu::executor::Runtime;
use ritsu::action::{ AsyncReadExt, AsyncWriteExt, fs };


fn main() -> io::Result<()> {
    let mut pool = Runtime::new()?;
    let handle = pool.handle();

    let fd = StdFile::open("./Cargo.toml")?;
    let stdout = StdFile::create("/dev/stdout")?;
    let mut fd = fs::File::from_std(fd, handle.clone()).into_io();
    let mut stdout = fs::File::from_std(stdout, handle).into_io();

    let fut = async move {
        let buf = fd.read().await?;
        stdout.write(buf.freeze()).await?;

        Ok(())
    };

    pool.run_until(fut)
}
