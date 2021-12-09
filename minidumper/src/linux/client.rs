use crate::Error;
use std::io::IoSlice;

pub struct Client {
    socket: uds::UnixSeqpacketConn,
}

impl Client {
    /// Creates a new client with the given name.
    pub fn with_name(name: impl AsRef<str>) -> Result<Self, Error> {
        let socket_addr =
            uds::UnixSocketAddr::from_abstract(name.as_ref()).map_err(|_err| Error::InvalidName)?;

        Ok(Self {
            socket: uds::UnixSeqpacketConn::connect_unix_addr(&socket_addr)?,
        })
    }

    /// Requests that the server generate a minidump for the specified crash
    /// context. This blocks until the server has finished writing the minidump.
    ///
    /// This uses a [`exception_handler::linux::CrashContext`] by reference as
    /// the size of it can be larger than one would want in an alternate stack
    /// handler, the use of a reference allows the context to be stored outside
    /// of the stack and heap to avoid that complication, but you may of course
    /// generate one however you like.
    pub fn request_dump(&self, crash_context: &super::CrashContext) -> Result<(), Error> {
        let crash_ctx_buffer = crash_context.as_bytes();

        let header = crate::Header {
            kind: 0,
            size: crash_ctx_buffer.len() as u32,
        };

        let header_buf = header.as_bytes();

        let io_bufs = [IoSlice::new(header_buf), IoSlice::new(crash_ctx_buffer)];
        self.socket.send_vectored(&io_bufs)?;

        exception_handler::debug_print!("waiting for dump to finish...");
        let mut ack = [0u8; 1];
        self.socket.recv(&mut ack)?;
        exception_handler::debug_print!("minidump creation acked");

        Ok(())
    }

    /// Sends a message to the server.
    pub fn send_message(
        &self,
        kind: std::num::NonZeroU32,
        buf: impl AsRef<[u8]>,
    ) -> Result<(), Error> {
        let buffer = buf.as_ref();

        let header = crate::Header {
            kind: kind.get(),
            size: buffer.len() as u32,
        };

        let io_bufs = [IoSlice::new(header.as_bytes()), IoSlice::new(buffer)];

        self.socket.send_vectored(&io_bufs)?;

        let mut ack = [0u8; 1];
        self.socket.recv(&mut ack)?;

        Ok(())
    }
}
