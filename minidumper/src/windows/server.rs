use super::uds;
use crate::Error;
use std::os::windows::io::IntoRawSocket;

pub struct Server {
    socket: uds::UnixListener,
}

struct ClientConn {
    /// The actual socket connection we established with accept
    socket: uds::UnixStream,

    /// The token we associated with the socket with the mio registry
    token: mio::Token,
}

impl ClientConn {
    #[inline]
    fn new(socket: uds::UnixStream, token: mio::Token) -> Self {
        Self { socket, token }
    }

    fn recv(&mut self) -> Option<(u32, Vec<u8>)> {
        use std::io::IoSliceMut;

        let mut hdr_buf = [0u8; std::mem::size_of::<crate::Header>()];
        let len = self.socket.peek(&mut hdr_buf).ok()?;

        if len == 0 {
            return None;
        }

        let header = crate::Header::from_bytes(&hdr_buf)?;
        let mut buffer = Vec::new();

        buffer.resize(header.size as usize, 0);

        self.socket
            .recv_vectored(&mut [IoSliceMut::new(&mut hdr_buf), IoSliceMut::new(&mut buffer)])
            .ok()?;

        Some((header.kind, buffer))
    }
}

#[inline]
fn as_mio(
    socket: &mut uds::UnixStream,
    func: impl FnOnce(&mut mio::net::TcpStream) -> std::io::Result<()>,
) -> Result<(), Error> {
    let mut mio_stream = socket.as_mio();
    let res = func(&mut mio_stream);
    mio_stream.into_raw_socket();
    res.map_err(Error::from)
}

impl Server {
    /// Creates a new server bound to the given name in an appropriate temporary directory.
    pub fn with_name(filename: impl AsRef<std::ffi::OsStr>) -> Result<Self, Error> {
        //let tp = super::make_temp_path(filename)?;

        Self::with_path(filename.as_ref())
    }

    /// Creates a new server with the given path.
    pub fn with_path(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        let socket = uds::UnixListener::bind(path)?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket })
    }

    /// Runs the server loop, accepting client connections and requests to
    /// create minidumps
    pub fn run(
        &self,
        handler: Box<dyn crate::ServerHandler>,
        shutdown: &std::sync::atomic::AtomicBool,
    ) -> Result<(), Error> {
        use mio::{Events, Interest, Poll, Token};

        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(10);

        let mut mio_listener = self.socket.as_mio();
        poll.registry()
            .register(&mut mio_listener, Token(0), Interest::READABLE)?;
        mio_listener.into_raw_socket();

        let mut clients = Vec::new();
        let mut id = 1;

        loop {
            if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            poll.poll(&mut events, Some(std::time::Duration::from_millis(10)))?;

            for event in events.iter() {
                if event.token().0 == 0 {
                    match self.socket.accept() {
                        Ok((mut accepted, _addr)) => {
                            let token = Token(id);
                            id += 1;

                            accepted.set_nonblocking(true)?;

                            as_mio(&mut accepted, |ms| {
                                poll.registry().register(ms, token, Interest::READABLE)
                            })?;

                            log::debug!("accepted connection {}", token.0);
                            clients.push(ClientConn::new(accepted, token));
                        }
                        Err(err) => {
                            log::error!("failed to accept socket connection: {}", err);
                        }
                    }
                } else if let Some(pos) = clients.iter().position(|cc| cc.token == event.token()) {
                    match clients[pos].recv() {
                        Some((0, crash_context)) => {
                            let mut cc = clients.swap_remove(pos);

                            let exit = match Self::handle_crash_request(
                                &crash_context,
                                handler.as_ref(),
                            ) {
                                Err(err) => {
                                    log::error!("failed to capture minidump: {}", err);
                                    false
                                }
                                Ok(exit) => {
                                    log::info!("captured minidump");
                                    exit
                                }
                            };

                            if let Err(e) =
                                as_mio(&mut cc.socket, |ms| poll.registry().deregister(ms))
                            {
                                log::error!("failed to deregister socket: {}", e);
                            }

                            if let Err(e) = cc.socket.send(&[1]) {
                                log::error!("failed to send ack: {}", e);
                            }

                            if exit {
                                return Ok(());
                            }
                        }
                        Some((kind, buffer)) => {
                            handler.on_message(
                                kind - 1, /* give the user back the original code they specified */
                                buffer,
                            );

                            if let Err(e) = clients[pos].socket.send(&[1]) {
                                log::error!("failed to send ack: {}", e);
                            }
                        }
                        None => {
                            log::debug!("client closed socket {}", pos);
                            let mut cc = clients.swap_remove(pos);

                            if let Err(e) =
                                as_mio(&mut cc.socket, |ms| poll.registry().deregister(ms))
                            {
                                log::error!("failed to deregister socket: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    fn handle_crash_request(
        buffer: &[u8],
        handler: &dyn crate::ServerHandler,
    ) -> Result<bool, Error> {
        use scroll::Pread;
        let dump_request: super::DumpRequest = buffer.pread(0)?;

        // MiniDumpWriteDump primarily uses `EXCEPTION_POINTERS` for its crash
        // context information, but inside that is an `EXCEPTION_RECORD`, which
        // is an internally linked list, so rather than recurse and allocate until
        // the end of that linked list, we just retrieve the actual pointer from
        // the client process, and inform the dump writer that they are pointers
        // to a different process, as MiniDumpWriteDump will internally read
        // the processes memory as needed
        let exception_pointers = dump_request.exception_pointers
            as *const windows_sys::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS;

        let cc = crash_context::CrashContext {
            exception_pointers,
            thread_id: dump_request.thread_id,
            exception_code: dump_request.exception_code,
        };

        let (mut minidump_file, minidump_path) = handler.create_minidump_file()?;

        let mut writer = minidump_writer::minidump_writer::MinidumpWriter::external_process(
            cc,
            dump_request.process_id,
        )?;

        let result = writer.dump(&mut minidump_file);

        // Notify the user handler about the minidump, even if we failed to write it
        Ok(handler.on_minidump_created(
            result
                .map(|contents| crate::MinidumpBinary {
                    file: minidump_file,
                    path: minidump_path,
                    contents: Vec::new(),
                })
                .map_err(crate::Error::from),
        ))
    }
}
