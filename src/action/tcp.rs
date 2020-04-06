use std::{ io, net, mem };
use std::task::{ Context, Poll };
use std::os::unix::io::{ AsRawFd, FromRawFd, RawFd };
use bytes::{ Bytes, BytesMut };
use socket2::{ SockAddr, Socket, Domain, Type, Protocol };
use io_uring::opcode::{ self, types };
use crate::action::{ AsHandle, AsyncRead, AsyncWrite };
use crate::action::iohelp::{ IoInner, IoState };
use crate::Handle;


pub struct TcpListener<H: Handle> {
    fd: net::TcpListener,
    sockaddr: mem::ManuallyDrop<Box<(libc::sockaddr, libc::socklen_t)>>,
    lock: bool,
    handle: H
}

pub struct TcpConnector<H: Handle> {
    sockaddr: mem::ManuallyDrop<Box<Option<SockAddr>>>,
    handle: H
}

pub struct TcpStream<H: Handle>(IoInner<net::TcpStream, H>);

impl<H: Handle> TcpListener<H> {
    pub fn from_std(fd: net::TcpListener, handle: H) -> TcpListener<H> {
        let sockaddr = mem::ManuallyDrop::new(Box::new((
            unsafe { mem::zeroed() },
            mem::size_of::<libc::sockaddr>() as _
        )));
        TcpListener {
            fd, sockaddr, handle,
            lock: false
        }
    }

    pub async fn accept(&mut self) -> io::Result<(TcpStream<H>, net::SocketAddr)> {
        let entry = opcode::Accept::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            &mut self.sockaddr.0,
            &mut self.sockaddr.1
        )
            .build();

        self.lock = true;
        let ret = unsafe { self.handle.push(entry).await };
        self.lock = false;
        let ret = ret?.result();

        if ret >= 0 {
            unsafe {
                let stream = net::TcpStream::from_raw_fd(ret);
                let addr = SockAddr::from_raw_parts(&self.sockaddr.0, self.sockaddr.1);

                let stream = TcpStream::from_std(stream, self.handle.clone());
                let addr = addr.as_std().unwrap();

                Ok((stream, addr))
            }
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }
}

impl<H: Handle> TcpConnector<H> {
    pub fn new(handle: H) -> TcpConnector<H> {
        TcpConnector {
            sockaddr: mem::ManuallyDrop::new(Box::new(None)),
            handle
        }
    }

    pub async fn connect(&mut self, addr: net::SocketAddr) -> io::Result<TcpStream<H>> {
        debug_assert!(self.sockaddr.is_none());

        let domain = match &addr {
            net::SocketAddr::V4(_) => Domain::ipv4(),
            net::SocketAddr::V6(_) => Domain::ipv6(),
        };
        let stream =
            Socket::new(domain, Type::stream(), Some(Protocol::tcp()))?;
        let sockaddr = self.sockaddr.get_or_insert(SockAddr::from(addr));

        let entry = opcode::Connect::new(
            types::Target::Fd(stream.as_raw_fd()),
            sockaddr.as_ptr() as *const _,
            sockaddr.len()
        )
            .build();

        let ret = unsafe { self.handle.push(entry).await };
        self.sockaddr.take();
        let ret = ret?.result();

        if ret >= 0 {
            Ok(TcpStream(IoInner {
                fd: stream.into_tcp_stream(),
                state: IoState::Empty,
                handle: self.handle.clone()
            }))
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }
}

impl<H: Handle> TcpStream<H> {
    pub fn from_std(fd: net::TcpStream, handle: H) -> TcpStream<H> {
        TcpStream(IoInner {
            fd, handle,
            state: IoState::Empty
        })
    }

    #[inline]
    pub async fn connect(addr: net::SocketAddr, handle: H) -> io::Result<TcpStream<H>> {
        TcpConnector::new(handle).connect(addr).await
    }
}

impl<H: Handle> AsyncRead for TcpStream<H> {
    #[inline]
    fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<BytesMut>> {
        self.0.poll_read(cx)
    }
}

impl<H: Handle> AsyncWrite for TcpStream<H> {
    #[inline]
    fn submit(&mut self, buf: Bytes) -> io::Result<()> {
        self.0.submit(buf)
    }

    #[inline]
    fn poll_flush(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Bytes>> {
        self.0.poll_flush(cx)
    }
}

impl<H: Handle> Drop for TcpListener<H> {
    fn drop(&mut self) {
        if !self.lock {
            unsafe {
                mem::ManuallyDrop::drop(&mut self.sockaddr);
            }
        }
    }
}

impl<H: Handle> Drop for TcpConnector<H> {
    fn drop(&mut self) {
        if self.sockaddr.is_none() {
            unsafe {
                mem::ManuallyDrop::drop(&mut self.sockaddr);
            }
        }
    }
}

impl<H: Handle> AsHandle for TcpListener<H> {
    type Handle = H;

    #[inline]
    fn as_handle(&self) -> &Self::Handle {
        &self.handle
    }
}

impl<H: Handle> AsHandle for TcpStream<H> {
    type Handle = H;

    #[inline]
    fn as_handle(&self) -> &Self::Handle {
        &self.0.handle
    }
}

impl<H: Handle> AsRawFd for TcpListener<H> {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl<H: Handle> AsRawFd for TcpStream<H> {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.0.fd.as_raw_fd()
    }
}
