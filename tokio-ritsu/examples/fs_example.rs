use std::io;
use std::thread;
use std::fs::File as StdFile;
use bytes::BytesMut;
use ritsu::executor::Runtime;
use ritsu::action::fs;
use ritsu::action::AsyncWriteExt;
use tokio_ritsu::Handle;


#[tokio::main]
async fn main() -> io::Result<()> {
    let (driver, handle) = Handle::new();

    thread::spawn(move || {
        let mut pool = Runtime::new().unwrap();
        let handle = pool.handle();
        pool.run_until(driver.register(handle))
            .unwrap();
    });

    let fd = StdFile::open("./Cargo.toml")?;
    let stdout = StdFile::create("/dev/stdout")?;
    let mut fd = fs::File::from_std(fd, handle.clone());
    let mut stdout = fs::File::from_std(stdout, handle).into_io();

    let fut = async move {
        let mut pos = 0;

        loop {
            let buf = fd.read_at(pos, BytesMut::with_capacity(64)).await?;

            if buf.is_empty() {
                break
            }

            pos += buf.len() as i64;
            stdout.write(buf.freeze()).await?;
        }

        Ok(()) as io::Result<()>
    };

    tokio::spawn(fut).await??;

    Ok(())
}
