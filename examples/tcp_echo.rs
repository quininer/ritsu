use std::{ io, net };
use futures_util::future::TryFutureExt;
use bytes::BytesMut;
use ritsu::executor::Runtime;
use ritsu::action::tcp;


fn main() -> io::Result<()> {
    let mut pool = Runtime::new()?;
    let spawner = pool.spawner();
    let handle = pool.handle();

    let listener = net::TcpListener::bind("127.0.0.1:1234")?;
    let mut listener = tcp::TcpListener::from_std(listener, handle);

    let fut = async move {
        loop {
            let (mut stream, addr) = listener.accept().await?;

            println!("accept: {:?}", addr);

            let copy_fut = async move {
                let mut buf = BytesMut::with_capacity(1024);

                loop {
                    let read_buf = stream.read(buf).await?;

                    if read_buf.is_empty() {
                        break
                    }

                    let mut read_buf = stream.write(read_buf).await?;
                    read_buf.clear();
                    buf = read_buf;
                }

                Ok(()) as io::Result<()>
            };

            spawner.spawn(copy_fut.unwrap_or_else(|err| eprintln!("{:?}", err)));
        }
    };

    pool.run_until(fut)
}
