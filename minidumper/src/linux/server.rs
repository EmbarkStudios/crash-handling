use crate::Error;

use uds::nonblocking::{UnixSeqpacketConn, UnixSeqpacketListener};

pub struct Server {
    listener: UnixSeqpacketListener,
}

struct ClientConn {
    /// The actual socket connection we established with accept
    socket: UnixSeqpacketConn,
    /// The key we associated with the socket
    key: usize,
}

impl ClientConn {
    #[inline]
    fn new(socket: UnixSeqpacketConn, key: usize) -> Self {
        Self { socket, key }
    }

    fn recv(&mut self, handler: &dyn crate::ServerHandler) -> Option<(u32, Vec<u8>)> {
        use std::io::IoSliceMut;

        let mut hdr_buf = [0u8; std::mem::size_of::<crate::Header>()];
        let (len, _trunc) = self.socket.peek(&mut hdr_buf).ok()?;

        if len == 0 {
            return None;
        }

        let header = crate::Header::from_bytes(&hdr_buf)?;
        let mut buffer = handler.message_alloc();

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
            listener: UnixSeqpacketListener::bind_unix_addr(&socket_addr)?,
        })
    }

    /// Runs the server loop, accepting client connections and requests to
    /// create minidumps
    pub fn run(
        &self,
        handler: Box<dyn crate::ServerHandler>,
        shutdown: &std::sync::atomic::AtomicBool,
    ) -> Result<(), Error> {
        use polling::{Event, Poller};

        let poll = Poller::new()?;
        let mut events = Vec::new();

        poll.add(&self.listener, Event::readable(0))?;

        let mut clients = Vec::new();
        let mut id = 1;

        loop {
            if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            events.clear();
            poll.wait(&mut events, Some(std::time::Duration::from_millis(10)))?;

            for event in events.iter() {
                if event.key == 0 {
                    match self.listener.accept_unix_addr() {
                        Ok((accepted, _addr)) => {
                            let key = id;
                            id += 1;

                            poll.add(&accepted, Event::readable(key))?;

                            log::debug!("accepted connection {}", key);
                            clients.push(ClientConn::new(accepted, key));
                        }
                        Err(err) => {
                            log::error!("failed to accept socket connection: {}", err);
                        }
                    }

                    // We need to reregister insterest every time
                    poll.modify(&self.listener, Event::readable(0))?;
                } else if let Some(pos) = clients.iter().position(|cc| cc.key == event.key) {
                    let deregister = match clients[pos].recv(handler.as_ref()) {
                        Some((0, crash_context)) => {
                            let cc = clients.swap_remove(pos);

                            let exit = match Self::handle_crash_request(
                                &cc.socket,
                                crash_context,
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

                            if let Err(e) = cc.socket.send(&[1]) {
                                log::error!("failed to send ack: {}", e);
                            }

                            if exit {
                                return Ok(());
                            }

                            Some(cc.socket)
                        }
                        Some((kind, buffer)) => {
                            handler.on_message(
                                kind - 1, /* give the user back the original code they specified */
                                buffer,
                            );

                            if let Err(e) = clients[pos].socket.send(&[1]) {
                                log::error!("failed to send ack: {}", e);
                            }

                            None
                        }
                        None => {
                            log::debug!("client closed socket {}", pos);
                            let cc = clients.swap_remove(pos);
                            Some(cc.socket)
                        }
                    };

                    if let Some(socket) = deregister {
                        if let Err(e) = poll.delete(&socket) {
                            log::error!("failed to deregister socket: {}", e);
                        }
                    } else {
                        poll.modify(&clients[pos].socket, Event::readable(clients[pos].key))?;
                    }
                }
            }
        }
    }

    fn handle_crash_request(
        socket: &UnixSeqpacketConn,
        buffer: Vec<u8>,
        handler: &dyn crate::ServerHandler,
    ) -> Result<bool, Error> {
        let peer_creds = socket.initial_peer_credentials()?;

        let pid = peer_creds.pid().ok_or(Error::UnknownClientPid)?;

        let cc = crash_context::CrashContext::from_bytes(&buffer).ok_or_else(|| {
            Error::from(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "client sent an incorrectly sized buffer",
            ))
        })?;

        let (mut minidump_file, minidump_path) = handler.create_minidump_file()?;

        let mut writer =
            minidump_writer::minidump_writer::MinidumpWriter::new(pid.get() as i32, cc.tid);
        writer.set_crash_context(minidump_writer::crash_context::CrashContext { inner: cc });

        let result = writer.dump(&mut minidump_file);

        // Notify the user handler about the minidump, even if we failed to write it
        Ok(handler.on_minidump_created(
            result
                .map(|contents| crate::MinidumpBinary {
                    file: minidump_file,
                    path: minidump_path,
                    contents,
                })
                .map_err(crate::Error::from),
        ))
    }
}
