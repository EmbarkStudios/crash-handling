use super::{Header, SocketName, Stream};
use crate::Error;
use std::io::IoSlice;

/// Client side of the connection, which runs in the process that may (or has)
/// crashed to communicate with an external watchdog process.
pub struct Client {
    socket: Stream,
}

impl Client {
    /// Creates a new client with the given name.
    ///
    /// # Errors
    ///
    /// The specified socket name is invalid, or a connection cannot be made
    /// with a server
    pub fn with_name<'scope>(name: impl Into<SocketName<'scope>>) -> Result<Self, Error> {
        let sn = name.into();

        cfg_if::cfg_if! {
            if #[cfg(any(target_os = "linux", target_os = "android"))] {
                let socket_addr = match sn {
                    SocketName::Path(path) => {
                        uds::UnixSocketAddr::from_path(path).map_err(|_err| Error::InvalidName)?
                    }
                    SocketName::Abstract(name) => {
                        uds::UnixSocketAddr::from_abstract(name).map_err(|_err| Error::InvalidName)?
                    }
                };

                let socket = Stream::connect_unix_addr(&socket_addr)?;
            } else {
                let SocketName::Path(path) = sn;
                let socket = Stream::connect(path)?;
            }
        }

        Ok(Self { socket })
    }

    /// Requests that the server generate a minidump for the specified crash
    /// context. This blocks until the server has finished writing the minidump.
    ///
    /// # Linux
    ///
    /// This uses a [`crash_context::CrashContext`] by reference as the size of
    /// it can be larger than one would want in an alternate stack handler, the
    /// use of a reference allows the context to be stored outside of the stack
    /// and heap to avoid that complication, though you may of course generate
    /// one however you like.
    ///
    /// # Windows
    ///
    /// This uses a [`crash_context::CrashContext`] by reference, as
    /// the crash context internally contains pointers into this process'
    /// memory that need to stay valid for the duration of the mindump creation.
    ///
    /// # Macos
    ///
    /// It is _highly_ recommended that you suspend all threads in the current
    /// process (other than the thread that executes this method) via
    /// [`thread_suspend`](https://developer.apple.com/documentation/kernel/1418833-thread_suspend)
    /// (apologies for the terrible documentation, blame Apple) before calling
    /// this method
    pub fn request_dump(&self, crash_context: &crash_context::CrashContext) -> Result<(), Error> {
        cfg_if::cfg_if! {
            if #[cfg(any(target_os = "linux", target_os = "android"))] {
                let crash_ctx_buffer = crash_context.as_bytes();
            } else if #[cfg(target_os = "windows")] {
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

                let crash_ctx_buffer = &buf[..written];
            } else if #[cfg(target_os = "macos")] {
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

                let crash_ctx_buffer = &buf[..written];
            }
        }

        let header = Header {
            kind: 0,
            size: crash_ctx_buffer.len() as u32,
        };

        let header_buf = header.as_bytes();

        let io_bufs = [IoSlice::new(header_buf), IoSlice::new(crash_ctx_buffer)];
        self.socket.send_vectored(&io_bufs)?;

        let mut ack = [0u8; 1];
        self.socket.recv(&mut ack)?;

        Ok(())
    }

    /// Sends a message to the server.
    ///
    /// This method is provided so that users can send their own application
    /// specific message to the watchdog process.
    ///
    /// There are no limits imposed by this method itself, but it is recommended
    /// to keep the message reasonably sized, eg. below 64KiB, as different
    /// targets will have different limits for the maximum payload that can be
    /// delivered.
    ///
    /// # Errors
    ///
    /// The write to the server fails
    pub fn send_message(&self, kind: u32, buf: impl AsRef<[u8]>) -> Result<(), Error> {
        let buffer = buf.as_ref();

        let header = Header {
            kind: kind + 1, // 0 is reserved for requesting a dump
            size: buffer.len() as u32,
        };

        let io_bufs = [IoSlice::new(header.as_bytes()), IoSlice::new(buffer)];

        self.socket.send_vectored(&io_bufs)?;

        // TODO: should we have an ACK? IPC is a (relatively) reliable communication
        // method, and reserving receives from the server for the exclusive
        // use of crash dumping, the main thing that users will care about, means
        // we reduce complication
        // let mut ack = [0u8; 1];
        // self.socket.recv(&mut ack)?;

        Ok(())
    }
}
