use std::{ io, net, mem };
use std::marker::Unpin;
use std::os::unix::io::{ AsRawFd, FromRawFd, RawFd };
use bytes::{ Buf, BufMut };
use socket2::{ SockAddr, Socket, Domain, Type, Protocol };
use io_uring::opcode::{ self, types };
use crate::util::{ iovecs, iovecs_mut };
use crate::action::AsHandle;
use crate::Handle;


pub struct TcpListener<H: Handle> {
    fd: net::TcpListener,
    handle: H
}

pub struct TcpStream<H: Handle> {
    fd: net::TcpStream,
    handle: H
}

impl<H: Handle> TcpListener<H> {
    pub fn from_std(fd: net::TcpListener, handle: H) -> TcpListener<H> {
        TcpListener { fd, handle }
    }

    pub async fn accept(&mut self) -> io::Result<(TcpStream<H>, net::SocketAddr)> {
        let mut sockaddr: Box<(libc::sockaddr, libc::socklen_t)> = Box::new((
            unsafe { mem::zeroed() },
            mem::size_of::<libc::sockaddr>() as _
        ));

        let entry = opcode::Accept::new(
            types::Target::Fd(self.fd.as_raw_fd()),
            &mut sockaddr.0,
            &mut sockaddr.1
        )
            .build();

        let wait = unsafe { self.handle.push(entry) };
        let ret = wait?.await.result();
        if ret >= 0 {
            unsafe {
                let stream = net::TcpStream::from_raw_fd(ret);
                let addr = SockAddr::from_raw_parts(&sockaddr.0, sockaddr.1);

                let stream = TcpStream::from_std(stream, self.handle.clone());
                let addr = addr.as_inet()
                    .map(net::SocketAddr::V4)
                    .or_else(|| addr.as_inet6().map(net::SocketAddr::V6))
                    .unwrap();

                Ok((stream, addr))
            }
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }
}

impl<H: Handle> TcpStream<H> {
    pub fn from_std(fd: net::TcpStream, handle: H) -> TcpStream<H> {
        TcpStream { fd, handle }
    }

    pub async fn connect(addr: net::SocketAddr, handle: H) -> io::Result<TcpStream<H>> {
        let domain = match &addr {
            net::SocketAddr::V4(_) => Domain::ipv4(),
            net::SocketAddr::V6(_) => Domain::ipv6(),
        };
        let sockaddr = Box::new(SockAddr::from(addr));
        let stream =
            Socket::new(domain, Type::stream(), Some(Protocol::tcp()))?;

        let entry = opcode::Connect::new(
            types::Target::Fd(stream.as_raw_fd()),
            sockaddr.as_ptr() as *const _,
            sockaddr.len()
        )
            .build();

        let wait = unsafe { handle.push(entry) };
        let ret = wait?.await.result();
        if ret >= 0 {
            drop(sockaddr);
            Ok(TcpStream {
                fd: stream.into_tcp_stream(),
                handle
            })
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    pub async fn read<B: BufMut + Unpin + 'static>(&mut self, mut buf: B) -> io::Result<B> {
        let mut bufs = iovecs_mut(&mut buf);

        let op = types::Target::Fd(self.fd.as_raw_fd());
        let entry = opcode::Readv::new(op, bufs.as_mut_ptr(), bufs.len() as _)
            .build();

        let wait = unsafe { self.handle.push(entry) };
        let ret = wait?.await.result();
        if ret >= 0 {
            unsafe {
                buf.advance_mut(ret as _);
            }

            drop(bufs);
            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-ret))
        }
    }

    pub async fn write<B: Buf + Unpin + 'static>(&mut self, mut buf: B) -> io::Result<B> {
        let mut bufs = iovecs(&buf);

        let op = types::Target::Fd(self.fd.as_raw_fd());
        let entry = opcode::Writev::new(op, bufs.as_mut_ptr(), bufs.len() as _)
            .build();

        let wait = unsafe { self.handle.push(entry) };
        let ret = wait?.await.result();
        if ret >= 0 {
            buf.advance(ret as _);
            drop(bufs);
            Ok(buf)
        } else {
            Err(io::Error::from_raw_os_error(-ret))
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
        &self.handle
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
        self.fd.as_raw_fd()
    }
}
