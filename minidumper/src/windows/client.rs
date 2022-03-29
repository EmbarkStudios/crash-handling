use super::uds::UnixStream;
use crate::Error;
use std::io;

pub struct Client {
    /// The pipe handle. Note that we don't use mio's `NamedPipe` here since the
    /// client side is fairly simple and importantly uses eg. `TransactNamedPipe`
    /// which might interfere with mio
    socket: UnixStream,
}

impl Client {
    /// Creates a new client that will attempt to connect to a socket at the given
    /// path.
    pub fn with_name(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        Ok(Self {
            socket: UnixStream::connect(path)?,
        })
    }

    /// Requests that the server generate a minidump for the specified crash
    /// context.
    ///
    /// This blocks until the server has finished writing the minidump.
    pub fn request_dump(
        &self,
        crash_context: &crash_context::CrashContext,
        _debug_print: bool,
    ) -> Result<(), Error> {
        use scroll::Pwrite;
        let mut buf = [0u8; 24];
        let written = buf.pwrite(
            super::DumpRequest {
                exception_pointers: crash_context.exception_pointers as _,
                thread_id: crash_context.thread_id,
                exception_code: crash_context.exception_code,
                process_id: std::process::id(),
            },
            0,
        )?;

        Self::transact(&self.socket, 0, &buf[..written])
    }

    /// Sends a message to the server.
    pub fn send_message(&self, kind: u32, buf: impl AsRef<[u8]>) -> Result<(), Error> {
        let buffer = buf.as_ref();
        Self::transact(&self.socket, kind + 1, buffer)
    }

    /// Sends a payload to the server and receives an ack
    fn transact(socket: &UnixStream, kind: u32, buffer: &[u8]) -> Result<(), Error> {
        let buffer_len = buffer.len() as u32;

        let header = crate::Header {
            kind,
            size: buffer_len,
        };

        let io_bufs = [
            io::IoSlice::new(header.as_bytes()),
            io::IoSlice::new(buffer),
        ];
        socket.send_vectored(&io_bufs)?;

        let mut ack = [0u8; 1];
        socket.recv(&mut ack)?;
        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {}
}
