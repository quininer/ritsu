use std::{ io, net, mem };
use std::os::unix::io::{ AsRawFd, FromRawFd, RawFd };
use bytes::{ Buf, BufMut, Bytes, BytesMut };
use socket2::{ SockAddr, Socket, Domain, Type, Protocol };
use io_uring::opcode::{ self, types };
use crate::action::AsHandle;
use crate::util::MaybeLock;
use crate::Handle;


pub struct TcpListener {
    fd: net::TcpListener,
    sockaddr: MaybeLock<Box<(libc::sockaddr, libc::socklen_t)>>,
    handle: Handle
}

pub struct TcpConnector {
    sockaddr: mem::ManuallyDrop<Box<Option<SockAddr>>>,
    handle: Handle
}

pub struct TcpStream {
    fd: net::TcpStream,
    handle: Handle
}

impl TcpListener {
    pub fn from_std(fd: net::TcpListener, handle: Handle) -> TcpListener {
        let sockaddr = MaybeLock::new(Box::new((
            unsafe { mem::zeroed() },
            mem::size_of::<libc::sockaddr>() as _
        )));
        TcpListener { fd, sockaddr, handle }
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
            unsafe { self.handle.push(entry) }
        };
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

impl TcpConnector {
    pub fn new(handle: Handle) -> TcpConnector {
        TcpConnector {
            sockaddr: mem::ManuallyDrop::new(Box::new(None)),
            handle
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
            unsafe { self.handle.push(entry) }
        };
        self.sockaddr.take();
        let ret = ret?.result();

        if ret >= 0 {
            Ok(TcpStream {
                fd: stream.into_tcp_stream(),
                handle: self.handle.clone()
            })
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }
}

impl TcpStream {
    pub fn from_std(fd: net::TcpStream, handle: Handle) -> TcpStream {
        TcpStream { fd, handle }
    }

    #[inline]
    pub async fn connect(addr: net::SocketAddr, handle: Handle) -> io::Result<TcpStream> {
        TcpConnector::new(handle).connect(addr).await
    }

    pub async fn read(&mut self, mut buf: BytesMut) -> io::Result<BytesMut> {
        let bytes = buf.bytes_mut();
        let entry = opcode::Read::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            bytes.as_mut_ptr() as *mut _,
            bytes.len() as _
        )
            .build();

        let ret = safety_await!{
            [ buf ];
            unsafe { self.handle.push(entry) }
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

    pub async fn write(&mut self, mut buf: Bytes) -> io::Result<Bytes> {
        let entry = opcode::Write::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            buf.as_ptr() as *const _,
            buf.len() as _
        )
            .build();

        let ret = safety_await!{
            [ buf ];
            unsafe { self.handle.push(entry) }
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

impl AsHandle for TcpListener {
    #[inline]
    fn as_handle(&self) -> &Handle {
        &self.handle
    }
}

impl AsHandle for TcpStream {
    #[inline]
    fn as_handle(&self) -> &Handle {
        &self.handle
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
