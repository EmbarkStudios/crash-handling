use crate::Error;

use uds::nonblocking::{UnixSeqpacketConn, UnixSeqpacketListener};

pub struct Server {
    socket: UnixSeqpacketListener,
}

struct ClientConn {
    /// The actual socket connection we established with accept
    socket: UnixSeqpacketConn,
    /// The token we associated with the socket with the mio registry
    token: mio::Token,
}

impl ClientConn {
    #[inline]
    fn new(socket: UnixSeqpacketConn, token: mio::Token) -> Self {
        Self { socket, token }
    }

    fn recv(&mut self) -> Option<(u32, Vec<u8>)> {
        use std::io::IoSliceMut;

        let mut hdr_buf = [0u8; std::mem::size_of::<crate::Header>()];
        let (len, _trunc) = self.socket.peek(&mut hdr_buf).ok()?;

        if len == 0 {
            return None;
        }

        let header = crate::Header::from_bytes(&hdr_buf)?;
        let mut buffer = Vec::new();
        buffer.resize(header.size as usize, 0);

        let (_len, _trunc) = self
            .socket
            .recv_vectored(&mut [IoSliceMut::new(&mut hdr_buf), IoSliceMut::new(&mut buffer)])
            .ok()?;

        Some((header.kind, buffer))
    }
}

impl Server {
    /// Creates a new server with the given name.
    pub fn with_name(name: impl AsRef<str>) -> Result<Self, Error> {
        let socket_addr =
            uds::UnixSocketAddr::from_abstract(name.as_ref()).map_err(|_err| Error::InvalidName)?;

        Ok(Self {
            socket: UnixSeqpacketListener::bind_unix_addr(&socket_addr)?,
        })
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
        poll.registry()
            .register(&mut &self.socket, Token(0), Interest::READABLE)?;

        let mut clients = Vec::new();
        let mut id = 1;

        loop {
            if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            poll.poll(&mut events, Some(std::time::Duration::from_millis(10)))?;

            for event in events.iter() {
                if event.token().0 == 0 {
                    match self.socket.accept_unix_addr() {
                        Ok((mut accepted, _addr)) => {
                            let token = Token(id);
                            id += 1;

                            poll.registry()
                                .register(&mut accepted, token, Interest::READABLE)?;

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

                            if let Err(err) = Self::handle_crash_request(
                                &cc.socket,
                                crash_context,
                                handler.as_ref(),
                            ) {
                                log::error!("failed to capture minidump: {}", err);
                            } else {
                                log::info!("captured minidump");
                            }

                            if let Err(e) = poll.registry().deregister(&mut cc.socket) {
                                log::error!("failed to deregister socket: {}", e);
                            }

                            if let Err(e) = cc.socket.send(&[1]) {
                                log::error!("failed to send ack: {}", e);
                            }
                        }
                        Some((kind, buffer)) => {
                            handler.on_message(kind, buffer);
                        }
                        None => {
                            log::info!("client closed socket");
                            let mut cc = clients.swap_remove(pos);

                            if let Err(e) = poll.registry().deregister(&mut cc.socket) {
                                log::error!("failed to deregister socket: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    fn handle_crash_request(
        socket: &UnixSeqpacketConn,
        buffer: Vec<u8>,
        handler: &dyn crate::ServerHandler,
    ) -> Result<(), Error> {
        let peer_creds = socket.initial_peer_credentials()?;

        let pid = peer_creds.pid().ok_or(Error::UnknownClientPid)?;

        let cc = super::CrashContext::from_bytes(&buffer).ok_or_else(|| {
            Error::from(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "client sent an incorrectly sized buffer",
            ))
        })?;

        let (mut minidump_file, minidump_path) = handler.create_minidump_file()?;

        let mut writer =
            minidump_writer::minidump_writer::MinidumpWriter::new(pid.get() as i32, cc.tid);
        writer.set_crash_context(cc);

        let result = writer.dump(&mut minidump_file);

        // Notify the user handler about the minidump, even if we failed to write it
        handler.on_minidump_created(
            result
                .map(|contents| crate::MinidumpBinary {
                    file: minidump_file,
                    path: minidump_path,
                    contents,
                })
                .map_err(crate::Error::from),
        );

        Ok(())
    }
}
