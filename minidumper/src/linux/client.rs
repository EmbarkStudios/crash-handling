use crate::Error;

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
        // This is fine since all members of the context are Sized
        #[allow(unsafe_code)]
        let buffer = unsafe {
            std::slice::from_raw_parts(
                (crash_context as *const super::CrashContext).cast::<u8>(),
                std::mem::size_of::<super::CrashContext>(),
            )
        };

        self.socket.send(buffer)?;

        let mut ack = [0u8; 1];
        self.socket.recv(&mut ack)?;

        Ok(())
    }
}
