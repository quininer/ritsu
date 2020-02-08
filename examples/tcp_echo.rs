use std::{ io, net };
use std::rc::Rc;
use std::cell::RefCell;
use futures_util::future::TryFutureExt;
use bytes::BytesMut;
use ritsu::executor::Runtime;
use ritsu::action::tcp;


fn main() -> io::Result<()> {
    let mut pool = Runtime::new()?;
    let spawner = pool.spawner();
    let handle = pool.handle();

    let bufpool = Rc::new(RefCell::new(vec![BytesMut::with_capacity(2048); 32]));
    let listener = net::TcpListener::bind("127.0.0.1:1234")?;
    let mut listener = tcp::TcpListener::from_std(listener, handle);

    let fut = async move {
        loop {
            let (mut stream, addr) = listener.accept().await?;
            let bufpool = bufpool.clone();

            println!("accept: {}", addr);

            let copy_fut = async move {
                let mut count = 0;

                loop {
                    stream.ready(tcp::Poll::IN).await?;

                    let mut buf = bufpool
                        .borrow_mut()
                        .pop()
                        .unwrap_or_else(|| BytesMut::with_capacity(2048));
                    buf.reserve(2048);

                    let buf = stream.read(buf).await?;

                    if buf.is_empty() {
                        println!("connect {} count: {}", addr, count);
                        bufpool
                            .borrow_mut()
                            .push(buf);
                        break
                    }

                    count += buf.len();

                    let mut buf = stream.write(buf).await?;

                    buf.clear();
                    bufpool
                        .borrow_mut()
                        .push(buf);
                }

                Ok(()) as io::Result<()>
            };

            spawner.spawn(copy_fut.unwrap_or_else(|err| eprintln!("{:?}", err)));
        }
    };

    pool.run_until(fut)
}
