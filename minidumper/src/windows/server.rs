use super::uds;
use crate::Error;

pub struct Server {
    listener: uds::UnixListener,
}

struct ClientConn {
    /// The actual socket connection we established with accept
    socket: uds::UnixStream,

    /// The token we associated with the socket with the mio registry
    key: usize,
    //token: mio::Token,
}

impl ClientConn {
    #[inline]
    fn new(socket: uds::UnixStream, key: usize) -> Self {
        Self { socket, key }
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

impl Server {
    /// Creates a new server with the given path.
    pub fn with_name(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        // Windows is not good about cleaning these up, so we assume the user,
        // who has control over what path they specify, is ok with deleting
        // previous sockets that weren't cleaned up
        let _res = std::fs::remove_file(path.as_ref());

        let listener = uds::UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener })
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
                    match self.listener.accept() {
                        Ok((accepted, _addr)) => {
                            let key = id;
                            id += 1;

                            accepted.set_nonblocking(true)?;
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
                    let deregister = match clients[pos].recv() {
                        Some((0, crash_context)) => {
                            let cc = clients.swap_remove(pos);

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

        let writer = minidump_writer::minidump_writer::MinidumpWriter::external_process(
            cc,
            dump_request.process_id,
        )?;

        let result = writer.dump(&mut minidump_file);

        // Notify the user handler about the minidump, even if we failed to write it
        Ok(handler.on_minidump_created(
            result
                .map(|_| crate::MinidumpBinary {
                    file: minidump_file,
                    path: minidump_path,
                    contents: Vec::new(),
                })
                .map_err(crate::Error::from),
        ))
    }
}
