use std::{ io, net, mem };
use std::os::unix::io::{ AsRawFd, FromRawFd, RawFd };
use bytes::{ Buf, BufMut };
use socket2::{ SockAddr, Socket, Domain, Type, Protocol };
use io_uring::opcode::{ self, types };
use crate::util::MaybeLock;
use crate::handle;


pub struct TcpListener {
    fd: net::TcpListener,
    sockaddr: MaybeLock<Box<(libc::sockaddr, libc::socklen_t)>>
}

pub struct TcpConnector {
    sockaddr: mem::ManuallyDrop<Box<Option<SockAddr>>>
}

pub struct TcpStream {
    fd: net::TcpStream
}

impl TcpListener {
    pub fn from_std(fd: net::TcpListener) -> TcpListener {
        let sockaddr = MaybeLock::new(Box::new((
            unsafe { mem::zeroed() },
            mem::size_of::<libc::sockaddr>() as _
        )));
        TcpListener { fd, sockaddr }
    }

    pub async fn accept(&mut self) -> io::Result<(TcpStream, net::SocketAddr)> {
        let entry = opcode::Accept::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            &mut self.sockaddr.0,
            &mut self.sockaddr.1
        )
            .build();

        let ret = safety_await!{
            ( self.sockaddr );
            unsafe { handle::push(entry) }
        };
        let ret = ret?.result();

        if ret >= 0 {
            unsafe {
                let stream = net::TcpStream::from_raw_fd(ret);
                let addr = SockAddr::from_raw_parts(&self.sockaddr.0, self.sockaddr.1);

                let stream = TcpStream::from_std(stream);
                let addr = addr.as_std().unwrap();

                Ok((stream, addr))
            }
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }
}

impl TcpConnector {
    pub fn new() -> TcpConnector {
        TcpConnector {
            sockaddr: mem::ManuallyDrop::new(Box::new(None))
        }
    }

    pub async fn connect(&mut self, addr: net::SocketAddr) -> io::Result<TcpStream> {
        assert!(self.sockaddr.is_none());

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

        let ret = safety_await!{
            unsafe { handle::push(entry) }
        };
        self.sockaddr.take();
        let ret = ret?.result();

        if ret >= 0 {
            Ok(TcpStream { fd: stream.into_tcp_stream() })
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }
}

impl TcpStream {
    pub fn from_std(fd: net::TcpStream) -> TcpStream {
        TcpStream { fd }
    }

    #[inline]
    pub async fn connect(addr: net::SocketAddr) -> io::Result<TcpStream> {
        TcpConnector::new().connect(addr).await
    }

    pub async fn read<B: BufMut + 'static>(&mut self, mut buf: B) -> io::Result<B> {
        let bytes = buf.bytes_mut();
        let entry = opcode::Read::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            bytes.as_mut_ptr() as *mut _,
            bytes.len() as _
        )
            .build();

        let ret = safety_await!{
            [ buf ];
            unsafe { handle::push(entry) }
        };

        let ret = ret?.result();

        if ret >= 0 {
            unsafe {
                buf.advance_mut(ret as _);
            }

            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    pub async fn write<B: Buf + 'static>(&mut self, mut buf: B) -> io::Result<B> {
        let bytes = buf.bytes();
        let entry = opcode::Write::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            bytes.as_ptr() as *const _,
            bytes.len() as _
        )
            .build();

        let ret = safety_await!{
            [ buf ];
            unsafe { handle::push(entry) }
        };
        let ret = ret?.result();

        if ret >= 0 {
            buf.advance(ret as _);
            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }
}

impl Drop for TcpConnector {
    fn drop(&mut self) {
        if self.sockaddr.is_none() {
            unsafe {
                mem::ManuallyDrop::drop(&mut self.sockaddr);
            }
        }
    }
}

impl AsRawFd for TcpListener {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl AsRawFd for TcpStream {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
