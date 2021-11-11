use crate::Error;

use uds::nonblocking::UnixSeqpacketListener;

pub struct Server {
    socket: UnixSeqpacketListener,
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

            poll.poll(&mut events, None)?;

            for event in events.iter() {
                if event.token().0 == 0 {
                    match self.socket.accept_unix_addr() {
                        Ok((mut accepted, _addr)) => {
                            let token = Token(id);
                            id += 1;

                            poll.registry()
                                .register(&mut accepted, token, Interest::READABLE)?;

                            clients.push((accepted, token));
                        }
                        Err(err) => {
                            println!("failed to accept socket connection: {}", err);
                        }
                    }
                } else {
                    if let Some(pos) = clients
                        .iter()
                        .position(|(_, token)| *token == event.token())
                    {
                        let (mut socket, _) = clients.swap_remove(pos);

                        if let Err(err) = Self::handle_crash_request(&socket, handler.as_ref()) {
                            println!("failed to capture minidump: {}", err);
                        } else {
                            println!("captured minidump");
                        }

                        if let Err(e) = poll.registry().deregister(&mut socket) {
                            println!("failed to deregister socket: {}", e);
                        }
                    }
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn handle_crash_request(
        socket: &uds::nonblocking::UnixSeqpacketConn,
        handler: &dyn crate::ServerHandler,
    ) -> Result<(), Error> {
        let mut crash_context_buffer = [0u8; std::mem::size_of::<super::CrashContext>()];

        let (len, _all) = socket.recv(&mut crash_context_buffer)?;

        if len != crash_context_buffer.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "client sent an incorrectly sized buffer",
            )
            .into());
        }

        let peer_creds = socket.initial_peer_credentials()?;

        let pid = peer_creds.pid().ok_or_else(|| Error::UnknownClientPid)?;

        #[allow(unsafe_code)]
        let cc = unsafe { (*crash_context_buffer.as_ptr().cast::<super::CrashContext>()).clone() };

        let mut minidump_file = handler.create_minidump_file()?;

        let mut writer =
            minidump_writer_linux::minidump_writer::MinidumpWriter::new(pid.get() as i32, cc.tid);
        writer.set_crash_context(cc);

        let result = writer.dump(&mut minidump_file);

        // Notify the user handler about the minidump, even if we failed to write it
        handler.on_minidump_created(
            result
                .map(|vec| (minidump_file, vec))
                .map_err(crate::Error::from),
        );

        Ok(())
    }
}
