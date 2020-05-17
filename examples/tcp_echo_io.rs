use std::{ io, net };
use std::rc::Rc;
use std::cell::RefCell;
use futures_util::future::TryFutureExt;
use tokio::io::copy;
use tokio::io::AsyncWriteExt;
use ritsu::executor::Runtime;
use ritsu::action::{ tcp, io as rio };
use ritsu::action::poll::{ Poll, ReadyExt };


fn main() -> io::Result<()> {
    let mut pool = Runtime::new()?;
    let spawner = pool.spawner();

    let listener = net::TcpListener::bind("127.0.0.1:1234")?;
    let mut listener = tcp::TcpListener::from_std(listener);

    let fut = async move {
        loop {
            let (stream, addr) = listener.accept().await?;
            let mut stream = rio::AsyncBufIo::with_capacity(stream, 512);

            println!("accept: {}", addr);

            let copy_fut = async move {
                let (mut rh, mut wh) = stream.split_ref();
                let count = copy(&mut rh, &mut wh).await?;

                println!("connect {} count: {}", addr, count);

                Ok(()) as io::Result<()>
            };

            spawner.spawn(copy_fut.unwrap_or_else(|err| eprintln!("{:?}", err)));
        }
    };

    pool.run_until(fut)
}
