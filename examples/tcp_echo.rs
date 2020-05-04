use std::{ io, net };
use std::rc::Rc;
use std::cell::RefCell;
use futures_util::future::TryFutureExt;
use bytes::{ Buf, BytesMut };
use ritsu::executor::Runtime;
use ritsu::action::tcp;
use ritsu::action::poll::{ Poll, ReadyExt };


fn main() -> io::Result<()> {
    let mut pool = Runtime::new()?;
    let spawner = pool.spawner();

    let listener = net::TcpListener::bind("127.0.0.1:1234")?;
    let mut listener = tcp::TcpListener::from_std(listener);
    let bufpool = Rc::new(RefCell::new(Vec::with_capacity(64)));

    let fut = async move {
        loop {
            let (mut stream, addr) = listener.accept().await?;
            let bufpool = bufpool.clone();

            println!("accept: {}", addr);

            let copy_fut = async move {
                let mut count = 0;

                loop {
                    stream.ready(Poll::READABLE).await?;

                    let mut buf = bufpool
                        .borrow_mut()
                        .pop()
                        .unwrap_or_else(BytesMut::new);
                    if buf.capacity() < 2048 {
                        buf.reserve(2048 - buf.capacity());
                    }

                    let buf = stream
                        .read(buf)
                        .await?;

                    if buf.is_empty() {
                        bufpool
                            .borrow_mut()
                            .push(buf);
                        break
                    }

                    let mut buf = buf.freeze();
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
