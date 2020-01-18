use std::io;
use std::fs::File as StdFile;
use ritsu::executor::Runtime;
use ritsu::action::fs;


fn main() -> io::Result<()> {
    let mut pool = Runtime::new()?;
    let handle = pool.handle();

    let fd = StdFile::open("./Cargo.toml")?;
    let fd = fs::File::from_std(fd, handle);

    let fut = async move {
        let buf = fd.read(vec![0; 24]).await?;

        println!("{}", String::from_utf8_lossy(&buf));

        Ok(())
    };

    pool.run_until(fut)
}
