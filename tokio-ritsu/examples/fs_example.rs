use std::io;
use std::thread;
use std::fs::File as StdFile;
use tokio::runtime;
use ritsu::executor::Runtime;
use ritsu::action::fs;
use tokio_ritsu::Handle;



fn main() -> io::Result<()> {
    let mut pool = Runtime::<Handle>::new().unwrap();
    let (driver, handle) = Handle::from(pool.raw_handle());

    thread::spawn(move || {
        let mut runtime = runtime::Builder::new()
            .basic_scheduler()
            .build()
            .unwrap();

        let fd = StdFile::open("./Cargo.toml").unwrap();
        let mut fd = fs::File::from_std(fd, handle);

        runtime.block_on(async {
            let fut = fd.read_at(0, Vec::with_capacity(30));

            fn is_send_sync<T: Send + Sync>(_: &T) {}
            is_send_sync(&fut);

            let buf = fut.await.unwrap();

            println!("{}", String::from_utf8_lossy(&buf));
        });
    });

    pool.run_until(driver)?;

    Ok(())
}
