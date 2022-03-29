//! Implements support for Unix domain sockets for Windows. This should be moved
//! into the `uds` crate or `mio` or some other crate, but all of the ones I have
//! found currently are outdated prototypes

use std::{
    io,
    os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket},
};
use windows_sys::Win32::{
    Foundation::{self as found, HANDLE},
    Networking::WinSock as ws,
};

// copy of mio's init since it isn't publicly exposed
pub(crate) fn init() {
    static INIT: parking_lot::Once = parking_lot::Once::new();
    INIT.call_once(|| {
        // Let standard library call `WSAStartup` for us, we can't do it
        // ourselves because otherwise using any type in `std::net` would panic
        // when it tries to call `WSAStartup` a second time.
        drop(std::net::UdpSocket::bind("127.0.0.1:0"));
    });
}

#[inline]
fn last_socket_error() -> io::Error {
    // SAFETY: syscall
    io::Error::from_raw_os_error(unsafe { ws::WSAGetLastError() })
}

pub(crate) struct UnixSocketAddr {
    addr: ws::sockaddr_un,
    len: i32,
}

impl UnixSocketAddr {
    pub(crate) fn from_path(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let path_bytes = path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "path is not utf-8"))?
            .as_bytes();

        let mut sock_addr = ws::sockaddr_un {
            sun_family: ws::AF_UNIX,
            sun_path: [0u8; 108],
        };

        if path_bytes.len() >= sock_addr.sun_path.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "specified path is too long",
            ));
        }

        sock_addr.sun_path[..path_bytes.len()].copy_from_slice(path_bytes);
        Self::from_parts(
            sock_addr,
            (2 /* family */ + path_bytes.len() + 1/* path len + null */) as i32,
        )
    }

    #[inline]
    fn from_parts(addr: ws::sockaddr_un, len: i32) -> io::Result<Self> {
        if addr.sun_family != ws::AF_UNIX {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "socket address is not a unix domain socket",
            ))
        } else {
            Ok(Self { addr, len })
        }
    }
}

struct Socket(ws::SOCKET);

impl Socket {
    pub fn new() -> io::Result<Socket> {
        // SAFETY: syscall
        let socket = unsafe {
            ws::WSASocketW(
                ws::AF_UNIX as i32,
                ws::SOCK_STREAM as i32,
                0,
                std::ptr::null_mut(),
                0,
                ws::WSA_FLAG_OVERLAPPED,
            )
        };

        if socket == ws::INVALID_SOCKET {
            Err(last_socket_error())
        } else {
            let socket = Self(socket);
            socket.set_no_inherit()?;
            Ok(socket)
        }
    }

    fn accept(&self, storage: *mut ws::SOCKADDR, len: &mut i32) -> io::Result<Self> {
        // SAFETY: syscall
        let socket = unsafe { ws::accept(self.0, storage, len) };

        if socket == ws::INVALID_SOCKET {
            Err(last_socket_error())
        } else {
            let socket = Self(socket);
            socket.set_no_inherit()?;
            Ok(socket)
        }
    }

    #[inline]
    fn set_no_inherit(&self) -> io::Result<()> {
        // SAFETY: syscall
        if unsafe { found::SetHandleInformation(self.0 as HANDLE, found::HANDLE_FLAG_INHERIT, 0) }
            == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn recv_with_flags(&self, buf: &mut [u8], flags: i32) -> io::Result<usize> {
        // On unix when a socket is shut down all further reads return 0, so we
        // do the same on windows to map a shut down socket to returning EOF.
        let length = std::cmp::min(buf.len(), i32::MAX as usize) as i32;
        // SAFETY: syscall
        let result = unsafe {
            ws::recv(
                self.as_raw_socket() as _,
                buf.as_mut_ptr().cast(),
                length,
                flags,
            )
        };

        match result {
            ws::SOCKET_ERROR => {
                let error = unsafe { ws::WSAGetLastError() };

                if error == ws::WSAESHUTDOWN {
                    Ok(0)
                } else {
                    Err(io::Error::from_raw_os_error(error))
                }
            }
            _ => Ok(result as usize),
        }
    }

    fn recv_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        // On unix when a socket is shut down all further reads return 0, so we
        // do the same on windows to map a shut down socket to returning EOF.
        let length = std::cmp::min(bufs.len(), u32::MAX as usize) as u32;
        let mut nread = 0;
        let mut flags = 0;
        // SAFETY: syscall
        let result = unsafe {
            ws::WSARecv(
                self.as_raw_socket() as _,
                bufs.as_mut_ptr() as *mut ws::WSABUF,
                length,
                &mut nread,
                &mut flags,
                std::ptr::null_mut(),
                None,
            )
        };

        if result == 0 {
            Ok(nread as usize)
        } else {
            // SAFETY: syscall
            let error = unsafe { ws::WSAGetLastError() };

            if error == ws::WSAESHUTDOWN {
                Ok(0)
            } else {
                Err(io::Error::from_raw_os_error(error))
            }
        }
    }

    fn send_vectored(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        let length = std::cmp::min(bufs.len(), u32::MAX as usize) as u32;
        let mut nwritten = 0;
        // SAFETY: syscall
        let result = unsafe {
            ws::WSASend(
                self.as_raw_socket() as _,
                bufs.as_ptr() as *const ws::WSABUF as *mut _,
                length,
                &mut nwritten,
                0,
                std::ptr::null_mut(),
                None,
            )
        };

        if result == 0 {
            Ok(nwritten as usize)
        } else {
            Err(last_socket_error())
        }
    }
}

impl AsRawSocket for Socket {
    fn as_raw_socket(&self) -> RawSocket {
        self.0 as RawSocket
    }
}

impl FromRawSocket for Socket {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        Self(sock as ws::SOCKET)
    }
}

impl IntoRawSocket for Socket {
    fn into_raw_socket(self) -> RawSocket {
        let ret = self.0 as RawSocket;
        std::mem::forget(self);
        ret
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        // SAFETY: syscall
        let _ = unsafe { ws::closesocket(self.0) };
    }
}

/// A Unix domain socket server
pub(crate) struct UnixListener(Socket);

impl UnixListener {
    pub(crate) fn bind(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        init();

        let inner = Socket::new()?;
        let addr = UnixSocketAddr::from_path(path.as_ref())?;

        // SAFETY: syscall
        if unsafe {
            ws::bind(
                inner.as_raw_socket() as _,
                (&addr.addr as *const ws::sockaddr_un).cast(),
                addr.len as i32,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }

        // If we managed to bind, delete the file so that it removed once we
        // shutdown
        std::fs::remove_file(path).unwrap(); // TODO: ignore probably?

        // SAFETY: syscall
        if unsafe {
            ws::listen(inner.as_raw_socket() as _, 128 /* backlog */)
        } != 0
        {
            Err(last_socket_error())
        } else {
            Ok(Self(inner))
        }
    }

    pub(crate) fn accept(&self) -> io::Result<(UnixStream, UnixSocketAddr)> {
        let mut sock_addr = std::mem::MaybeUninit::<ws::sockaddr_un>::uninit();
        let mut len = std::mem::size_of::<ws::sockaddr_un>() as i32;

        let sock = self.0.accept(sock_addr.as_mut_ptr().cast(), &mut len)?;
        // SAFETY: should have been initialized if accept succeeded
        let addr = UnixSocketAddr::from_parts(unsafe { sock_addr.assume_init() }, len)?;

        Ok((UnixStream(sock), addr))
    }

    pub(crate) fn as_mio(&self) -> mio::net::TcpListener {
        // SAFETY: trait method is unsafe, but not really unsafe
        unsafe { mio::net::TcpListener::from_raw_socket(self.as_raw_socket()) }
    }
}

impl AsRawSocket for UnixListener {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.as_raw_socket()
    }
}

impl FromRawSocket for UnixListener {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        Self(Socket::from_raw_socket(sock))
    }
}

impl IntoRawSocket for UnixListener {
    fn into_raw_socket(self) -> RawSocket {
        let ret = self.0.as_raw_socket();
        std::mem::forget(self);
        ret
    }
}

/// A Unix doman socket stream
pub(crate) struct UnixStream(Socket);

impl UnixStream {
    pub(crate) fn connect(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        init();

        let inner = Socket::new()?;
        let addr = UnixSocketAddr::from_path(path)?;

        // SAFETY: syscall
        if unsafe {
            ws::connect(
                inner.as_raw_socket() as _,
                (&addr.addr as *const ws::sockaddr_un).cast(),
                addr.len as i32,
            )
        } != 0
        {
            Err(last_socket_error())
        } else {
            Ok(Self(inner))
        }
    }

    #[inline]
    pub(crate) fn peek(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.recv_with_flags(buf, ws::MSG_PEEK as i32)
    }

    #[inline]
    pub(crate) fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.recv_vectored(&mut [io::IoSliceMut::new(buf)])
    }

    #[inline]
    pub(crate) fn recv_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        self.0.recv_vectored(bufs)
    }

    #[inline]
    pub(crate) fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.send_vectored(&[io::IoSlice::new(buf)])
    }

    #[inline]
    pub(crate) fn send_vectored(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.0.send_vectored(bufs)
    }

    pub(crate) fn as_mio(&self) -> mio::net::TcpStream {
        // SAFETY: trait method is unsafe, but not really unsafe
        unsafe { mio::net::TcpStream::from_raw_socket(self.as_raw_socket()) }
    }
}

impl AsRawSocket for UnixStream {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.as_raw_socket()
    }
}

impl FromRawSocket for UnixStream {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        Self(Socket::from_raw_socket(sock))
    }
}

impl IntoRawSocket for UnixStream {
    fn into_raw_socket(self) -> RawSocket {
        let ret = self.0.as_raw_socket();
        std::mem::forget(self);
        ret
    }
}
