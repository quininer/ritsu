use std::{ io, net };
use std::convert::Infallible;
use futures_util::future::TryFutureExt;
use hyper::server::conn::Http;
use hyper::{ Request, Response, Body };
use hyper::service::service_fn;
use ritsu::executor::Runtime;
use ritsu::action::{ tcp, io as rio };


async fn handle(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::from("Hello World")))
}

fn main() -> io::Result<()> {
    let mut pool = Runtime::new()?;
    let spawner = pool.spawner();

    let listener = net::TcpListener::bind("127.0.0.1:8008")?;
    let mut listener = tcp::TcpListener::from_std(listener);

    let fut = async move {
        loop {
            let (stream, addr) = listener.accept().await?;
            let stream = rio::AsyncBufIo::new(stream);

            println!("accept: {}", addr);

            let fut = async move {
                Http::new()
                    .serve_connection(stream, service_fn(handle)).await?;

                Ok(()) as hyper::Result<()>
            };

            spawner.spawn(fut.unwrap_or_else(|err| eprintln!("{:?}", err)));
        }
    };

    pool.run_until(fut)
}
