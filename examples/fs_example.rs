use std::io;
use std::fs::File as StdFile;
use ritsu::executor::Runtime;
use ritsu::action::{ fs2, AsyncRead, AsyncWrite };


fn main() -> io::Result<()> {
    let mut pool = Runtime::new()?;
    let handle = pool.handle();

    let fd = StdFile::open("./Cargo.toml")?;
    let stdout = StdFile::create("/dev/stdout")?;
    let mut fd = fs2::File::from_std(fd, handle.clone());
    let mut stdout = fs2::File::from_std(stdout, handle);

    let fut = async move {
        let buf = (&mut fd as &mut dyn AsyncRead).read().await?.unwrap();
        (&mut stdout as &mut dyn AsyncWrite).write(buf.freeze()).await?;

        Ok(())
    };

    pool.run_until(fut)
}
