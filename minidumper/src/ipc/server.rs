use super::{Connection, Header, Listener, SocketName};
use crate::Error;
use polling::{Event, Poller};
use std::time::Duration;

/// Server side of the connection, which runs in the watchdog process that is
/// meant to monitor the process where the [`super::Client`] resides
pub struct Server {
    listener: Listener,
    #[cfg(target_os = "macos")]
    port: crash_context::ipc::Server,
    /// For abstract sockets, we don't have to worry about cleanup as it is
    /// handled by the OS, but on Windows and MacOS we need to clean them up
    /// manually. We basically rely on the crash watchdog program this Server
    /// is running in to exit cleanly, which should be mostly true, but we
    /// may need to harden this code if people experience issues with socket
    /// paths not being cleaned up reliably
    socket_path: Option<std::path::PathBuf>,
}

struct ClientConn {
    /// The actual socket connection we established with accept
    socket: Connection,
    /// The key we associated with the socket
    key: usize,
    /// We pair the pid of the client process so that we know which connection
    /// to drop when a crash is received on the mach port
    #[cfg(target_os = "macos")]
    pid: Option<u32>,
}

impl ClientConn {
    fn recv(&mut self, handler: &dyn crate::ServerHandler) -> Option<(u32, Vec<u8>)> {
        use std::io::IoSliceMut;

        let mut hdr_buf = [0u8; std::mem::size_of::<Header>()];
        cfg_if::cfg_if! {
            if #[cfg(any(target_os = "linux", target_os = "android"))] {
                let (len, _trunc) = self.socket.peek(&mut hdr_buf).ok()?;
            } else {
                let len = self.socket.peek(&mut hdr_buf).ok()?;
            }
        }

        if len == 0 {
            return None;
        }

        let header = Header::from_bytes(&hdr_buf)?;
        let mut buffer = handler.message_alloc();

        buffer.resize(header.size as usize, 0);

        self.socket
            .recv_vectored(&mut [IoSliceMut::new(&mut hdr_buf), IoSliceMut::new(&mut buffer)])
            .ok()?;

        Some((header.kind, buffer))
    }
}

impl Server {
    /// Creates a new server with the given name.
    ///
    /// Note that in the case of a path socket name, this method always attempts
    /// to delete the specified path if it exists as both Windows and Macos have
    /// issues around cleaning up these files if the process the server runs in
    /// aborts abnormally.
    ///
    /// # Errors
    ///
    /// The provided socket name is invalid, or the listener socket was unable
    /// to be bound to the specified socket name.
    pub fn with_name<'scope>(name: impl Into<SocketName<'scope>>) -> Result<Self, Error> {
        let sn = name.into();

        #[allow(irrefutable_let_patterns)]
        let socket_path = if let SocketName::Path(path) = &sn {
            // There seems to be a bug, at least on Windows, where checking for
            // the existence of the file path will actually fail even if the file
            // is actually there, so we just unconditionally remove the path
            let _res = std::fs::remove_file(path);

            Some(std::path::PathBuf::from(path))
        } else {
            None
        };

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

                let listener = Listener::bind_unix_addr(&socket_addr)?;
            } else if #[cfg(target_os = "windows")] {
                let SocketName::Path(path) = sn;
                let listener = Listener::bind(path)?;
                listener.set_nonblocking(true)?;
            } else if #[cfg(target_os = "macos")] {
                let SocketName::Path(path) = sn;
                let listener = Listener::bind(path)?;
                listener.set_nonblocking(true)?;

                // Note that sun_path is limited to 108 characters including null,
                // while a mach port name is limited to 128 including null, so
                // the length is already effectively checked here
                let port_name = std::ffi::CString::new(path.to_str().ok_or(Error::InvalidPortName)?).map_err(|_err| Error::InvalidPortName)?;
                let port = crash_context::ipc::Server::create(&port_name)?;
            } else {
                compile_error!("unimplemented target platform");
            }
        }

        Ok(Self {
            listener,
            #[cfg(target_os = "macos")]
            port,
            socket_path,
        })
    }

    /// Runs the server loop, accepting client connections and receiving IPC
    /// messages.
    ///
    /// # Errors
    ///
    /// This method uses basic I/O event notification via [`polling`] which
    /// can fail for a number of different reasons
    pub fn run(
        &mut self,
        handler: Box<dyn crate::ServerHandler>,
        shutdown: &std::sync::atomic::AtomicBool,
    ) -> Result<(), Error> {
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
            poll.wait(&mut events, Some(Duration::from_millis(10)))?;

            #[cfg(target_os = "macos")]
            if self.check_mach_port(&poll, &mut clients, handler.as_ref())? {
                return Ok(());
            }

            for event in events.iter() {
                if event.key == 0 {
                    match self.listener.accept_unix_addr() {
                        Ok((accepted, _addr)) => {
                            let key = id;
                            id += 1;

                            poll.add(&accepted, Event::readable(key))?;

                            log::debug!("accepted connection {}", key);
                            clients.push(ClientConn {
                                socket: accepted,
                                key,
                                #[cfg(target_os = "macos")]
                                pid: None,
                            });
                        }
                        Err(err) => {
                            log::error!("failed to accept socket connection: {}", err);
                        }
                    }

                    // We need to reregister insterest every time
                    poll.modify(&self.listener, Event::readable(0))?;
                } else if let Some(pos) = clients.iter().position(|cc| cc.key == event.key) {
                    let deregister = match clients[pos].recv(handler.as_ref()) {
                        Some((0, buffer)) => {
                            cfg_if::cfg_if! {
                                if #[cfg(target_os = "macos")] {
                                    use scroll::Pread;
                                    let pid: u32 = buffer.pread(0)?;
                                    clients[pos].pid = Some(pid);

                                    if let Err(e) = clients[pos].socket.send(&[1]) {
                                        log::error!("failed to send ack: {}", e);
                                    }

                                    None
                                } else {
                                    let cc = clients.swap_remove(pos);

                                    cfg_if::cfg_if! {
                                        if #[cfg(any(target_os = "linux", target_os = "android"))] {
                                            let peer_creds = cc.socket.initial_peer_credentials()?;

                                            let pid = peer_creds.pid().ok_or(Error::UnknownClientPid)?;

                                            let crash_ctx = crash_context::CrashContext::from_bytes(&buffer).ok_or_else(|| {
                                                Error::from(std::io::Error::new(
                                                    std::io::ErrorKind::InvalidData,
                                                    "client sent an incorrectly sized buffer",
                                                ))
                                            })?;

                                            // Validate that the crash info and the socket agree on the pid
                                            if pid.get() != crash_ctx.pid as u32 {
                                                return Err(Error::UnknownClientPid);
                                            }
                                        } else if #[cfg(target_os = "windows")] {
                                            use scroll::Pread;
                                            let dump_request: super::DumpRequest = buffer.pread(0)?;

                                            // MiniDumpWriteDump primarily uses `EXCEPTION_POINTERS` for its crash
                                            // context information, but inside that is an `EXCEPTION_RECORD`, which
                                            // is an internally linked list, so rather than recurse and allocate until
                                            // the end of that linked list, we just retrieve the actual pointer from
                                            // the client process, and inform the dump writer that they are pointers
                                            // to a different process, as MiniDumpWriteDump will internally read
                                            // the processes memory as needed
                                            let exception_pointers = dump_request.exception_pointers as *const std::ffi::c_void;

                                            let crash_ctx = crash_context::CrashContext {
                                                exception_pointers,
                                                process_id: dump_request.process_id,
                                                thread_id: dump_request.thread_id,
                                                exception_code: dump_request.exception_code,
                                            };
                                        }
                                    }

                                    let exit =
                                        match Self::handle_crash_request(crash_ctx, handler.as_ref()) {
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
                            }
                        }
                        Some((kind, buffer)) => {
                            handler.on_message(
                                kind - 1, /* give the user back the original code they specified */
                                buffer,
                            );

                            // We only send acks for crash dump requests
                            // if let Err(e) = clients[pos].socket.send(&[1]) {
                            //     log::error!("failed to send ack: {}", e);
                            // }

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
        crash_context: crash_context::CrashContext,
        handler: &dyn crate::ServerHandler,
    ) -> Result<bool, Error> {
        cfg_if::cfg_if! {
            if #[cfg(any(target_os = "linux", target_os = "android"))] {
                let mut writer =
                    minidump_writer::minidump_writer::MinidumpWriter::new(crash_context.pid, crash_context.tid);
                writer.set_crash_context(minidump_writer::crash_context::CrashContext { inner: crash_context });
            } else if #[cfg(target_os = "windows")] {
                let writer = minidump_writer::minidump_writer::MinidumpWriter::new(crash_context)?;
            } else if #[cfg(target_os = "macos")] {
                let mut writer = minidump_writer::minidump_writer::MinidumpWriter::new(crash_context);
            }
        }

        let (mut minidump_file, minidump_path) = handler.create_minidump_file()?;
        let result = writer.dump(&mut minidump_file);

        // Notify the user handler about the minidump, even if we failed to write it
        Ok(handler.on_minidump_created(
            result
                .map(|_contents| crate::MinidumpBinary {
                    file: minidump_file,
                    path: minidump_path,
                    #[cfg(target_os = "windows")]
                    contents: Vec::new(),
                    #[cfg(not(target_os = "windows"))]
                    contents: _contents,
                })
                .map_err(crate::Error::from),
        ))
    }

    #[cfg(target_os = "macos")]
    fn check_mach_port(
        &mut self,
        poll: &Poller,
        clients: &mut Vec<ClientConn>,
        handler: &dyn crate::ServerHandler,
    ) -> Result<bool, Error> {
        // We use a really short timeout for receiving on the mach port since we check it
        // frequently rather than spawning a separate thread and blocking
        if let Some(mut rcc) = self
            .port
            .try_recv_crash_context(Some(Duration::from_millis(1)))?
        {
            // Try to find a client connection that matches the port sender
            let pos = clients
                .iter()
                .position(|cc| cc.pid == Some(rcc.pid))
                .ok_or(Error::UnknownClientPid)?;
            let cc = clients.swap_remove(pos);

            let exit = match Self::handle_crash_request(rcc.crash_context, handler) {
                Err(err) => {
                    log::error!("failed to capture minidump: {}", err);
                    false
                }
                Ok(exit) => {
                    log::info!("captured minidump");
                    exit
                }
            };

            if let Err(e) = rcc.acker.send_ack(1, Some(Duration::from_secs(2))) {
                log::error!("failed to send ack: {}", e);
            }

            if let Err(e) = poll.delete(&cc.socket) {
                log::error!("failed to deregister socket: {}", e);
            }

            Ok(exit)
        } else {
            Ok(false)
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(path) = self.socket_path.take() {
            let _res = std::fs::remove_file(path);
        }
    }
}
