use std::{ io, net };
use futures_util::future::TryFutureExt;
use bytes::Buf;
use ritsu::executor::Runtime;
use ritsu::action::{ AsyncReadExt, AsyncWriteExt };
use ritsu::action::tcp;
use ritsu::action::poll::{ Poll, ReadyExt };


fn main() -> io::Result<()> {
    let mut pool = Runtime::new()?;
    let spawner = pool.spawner();
    let handle = pool.handle();

    let listener = net::TcpListener::bind("127.0.0.1:1234")?;
    let mut listener = tcp::TcpListener::from_std(listener, handle);

    let fut = async move {
        loop {
            let (mut stream, addr) = listener.accept().await?;

            println!("accept: {}", addr);

            let copy_fut = async move {
                let mut count = 0;

                loop {
                    stream.ready(Poll::READABLE).await?;
                    let mut buf = stream
                        .read()
                        .await?
                        .freeze();

                    if buf.is_empty() {
                        break
                    }

                    count += buf.len();

                    while buf.has_remaining() {
                        buf = stream.write(buf).await?;
                    }
                }

                println!("connect {} count: {}", addr, count);

                Ok(()) as io::Result<()>
            };

            spawner.spawn(copy_fut.unwrap_or_else(|err| eprintln!("{:?}", err)));
        }
    };

    pool.run_until(fut)
}
