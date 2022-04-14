use crate::{write_stderr, Error};
use std::io;

pub struct Client {
    socket: uds::UnixSeqpacketConn,
}

impl Client {
    /// Creates a new client with the given name.
    pub fn with_name(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        let socket_addr =
            uds::UnixSocketAddr::from_path(path.as_ref()).map_err(|_err| Error::InvalidName)?;

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
    pub fn request_dump(
        &self,
        crash_context: &crash_context::CrashContext,
        debug_print: bool,
    ) -> Result<(), Error> {
        let mut buf = [0u8; 48];

        let (has_exception, kind, code, has_subcode, subcode) =
            if let Some(exc) = crash_context.exception {
                (
                    1,
                    exc.kind,
                    exc.code,
                    if exc.subcode.is_some() { 1 } else { 0 },
                    exc.subcode.unwrap_or_default(),
                )
            } else {
                (0, 0, 0, 0, 0)
            };

        use scroll::Pwrite;
        let written = buf.pwrite(
            super::DumpRequest {
                task: crash_context.task,
                thread: crash_context.thread,
                handler_thread: crash_context.handler_thread,
                has_exception,
                kind,
                code,
                has_subcode,
                subcode,
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
    fn transact(socket: &uds::UnixSeqpacketConn, kind: u32, buffer: &[u8]) -> Result<(), Error> {
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
