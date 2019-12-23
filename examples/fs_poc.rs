use std::io;
use std::fs::File as StdFile;
use ritsu::executor::LocalPool;
use ritsu::action::fs;


fn main() -> io::Result<()> {
    let mut pool = LocalPool::new()?;
    let handle = pool.handle();

    let fd = StdFile::open("./Cargo.toml")?;
    let fd = fs::File::from_std(fd, handle);

    let fut = async move {
        let (_, buf_result) = fs::read(fd, vec![0; 24]).await;
        let buf = buf_result?;

        println!("{}", String::from_utf8_lossy(&buf));

        Ok(())
    };

    pool.run_until(fut)
}
