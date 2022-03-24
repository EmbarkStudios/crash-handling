use super::{last_os_error, Message, OwnedHandle, Tags};
use crate::Error;
use mio::windows::NamedPipe;
use std::{
    io::{ErrorKind, Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle, RawHandle},
};
use windows_sys::Win32::{
    Foundation::{DuplicateHandle, DUPLICATE_CLOSE_SOURCE, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem as fs,
    System::{
        Pipes as pipe,
        Threading::{self as proc, CreateEventA, CreateMutexA, SetEvent},
    },
};

type ProtoMsg = Message<64>;

pub struct Server {
    pipe_name: String,
    /// Event used by clients to detect if the server shutsdown while they
    /// are waiting for a minidump to be generated
    alive_handle: OwnedHandle,
}

struct ClientConn {
    /// The actual client connection we established with connect
    pipe: NamedPipe,
    /// The token we associated with the socket with the mio registry
    token: mio::Token,
}

struct ClientInfo {
    process_id: u32,
    process: OwnedHandle,
    dump_generated: OwnedHandle,
}

struct Client {
    conn: ClientConn,
    info: Option<ClientInfo>,
}

enum ClientMsg {
    Tagged((Tags, ProtoMsg)),
    User((u32, Vec<u8>)),
}

impl Client {
    fn recv(&mut self) -> Option<ClientMsg> {
        let mut hdr_buf = [0u8; std::mem::size_of::<crate::Header>()];
        let mut read = 0;
        let mut left = 0;
        // SAFETY: syscall
        if unsafe {
            pipe::PeekNamedPipe(
                self.conn.pipe.as_raw_handle() as HANDLE,
                hdr_buf.as_mut_ptr().cast(),
                std::mem::size_of::<crate::Header>() as u32,
                &mut read,
                std::ptr::null_mut(),
                &mut left,
            )
        } == 0
            || read as usize <= hdr_buf.len()
        {
            return None;
        }

        let header = crate::Header::from_bytes(&hdr_buf)?;

        if let Ok(tag) = Tags::from_u32(header.kind) {
            let mut msg = ProtoMsg::default();
            let read = self.conn.pipe.read(msg.as_mut()).ok()?;
            msg.validate(read as u32).ok()?;

            Some(ClientMsg::Tagged((tag, msg)))
        } else {
            let mut buffer = Vec::new();
            buffer.resize(header.size as usize, 0);
            self.conn.pipe.read(&mut hdr_buf).ok()?;
            self.conn.pipe.read(&mut buffer).ok()?;

            Some(ClientMsg::User((
                header.kind - Tags::UserRequest as u32,
                buffer,
            )))
        }
    }
}

impl Server {
    /// Creates a new server with the given name.
    pub fn with_name(name: impl AsRef<str>) -> Result<Self, Error> {
        let pipe_name = format!("\\\\.\\pipe\\{}\0", name.as_ref());

        // SAFETY: syscall
        let alive_handle = unsafe {
            CreateMutexA(
                std::ptr::null(), // security attributes
                1,                // initial owner
                std::ptr::null(), // name
            )
        };

        if alive_handle == 0 {
            return Err(last_os_error().into());
        }

        Ok(Self {
            pipe_name,
            alive_handle: OwnedHandle::new(alive_handle),
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

        let mut id = 0;

        // Named pipes are incredibly annoying and don't follow the listen
        // model of sockets
        let mut create_pipe = |poll: &mut Poll| -> Result<ClientConn, Error> {
            // Note we do this manually as mio doesn't expose any options builder,
            // and we want to use message pipes, not byte pipes which is the only
            // one that mio provides
            // SAFETY: syscall
            let pipe_handle = unsafe {
                pipe::CreateNamedPipeA(
                    self.pipe_name.as_ptr(),
                    fs::FILE_FLAG_FIRST_PIPE_INSTANCE
                        | fs::PIPE_ACCESS_DUPLEX
                        | fs::FILE_FLAG_OVERLAPPED,
                    pipe::PIPE_TYPE_MESSAGE | pipe::PIPE_READMODE_MESSAGE,
                    1,    // max instances
                    1024, // out buffer size
                    1024, // in buffer size
                    0,    // default timeout
                    std::ptr::null(),
                )
            };

            if pipe_handle == INVALID_HANDLE_VALUE {
                return Err(last_os_error());
            }

            // SAFETY: trait function marked unsafe, but not really that unsafe
            let mut pipe = unsafe { NamedPipe::from_raw_handle(pipe_handle as RawHandle) };

            let token = Token(id);
            id += 1;

            poll.registry()
                .register(&mut pipe, token, Interest::WRITABLE | Interest::READABLE)?;

            // Immediately issue a connect, but WouldBlock is not a terminal failure,
            // unlike other errors
            if let Err(e) = pipe.connect() {
                if e.kind() != ErrorKind::WouldBlock {
                    return Err(e.into());
                }
            }

            Ok(ClientConn { pipe, token })
        };

        let disconnect = |mut conn: ClientConn, poll: &mut Poll| {
            if let Err(e) = poll.registry().deregister(&mut conn.pipe) {
                log::error!("failed to deregister pipe: {}", e);
            }

            if let Err(e) = conn.pipe.disconnect() {
                log::error!("failed to disconnect pipe: {}", e);
            }
        };

        let mut clients = Vec::new();
        // This is the "current" listener waiting for a client connection, once
        // a connection is established we have to create a new one to handle new
        // clients because pipes are lame
        let mut listener = create_pipe(&mut poll)?;

        loop {
            if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            poll.poll(&mut events, Some(std::time::Duration::from_millis(10)))?;

            for event in events.iter() {
                if listener.token == event.token() {
                    log::debug!("accepted connection {}", listener.token.0);

                    // New client connected, so push it into the client list and
                    // create a new socket to "listen" for client connections on :(
                    let new_listener = create_pipe(&mut poll)?;
                    let mut connected = std::mem::replace(&mut listener, new_listener);

                    // Deregister write interest for the new connection as it will
                    // essentially always be ready for writes
                    poll.registry().reregister(
                        &mut connected.pipe,
                        connected.token,
                        Interest::READABLE,
                    )?;
                    clients.push(Client {
                        conn: connected,
                        info: None,
                    });
                } else if let Some(pos) =
                    clients.iter().position(|cc| cc.conn.token == event.token())
                {
                    match clients[pos].recv() {
                        Some(ClientMsg::Tagged((Tags::RequestDump, crash_context))) => {
                            let mut cc = clients.swap_remove(pos);

                            let exit = match Self::handle_crash_request(
                                &cc,
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

                            // TODO: Just send a message back on the pipe instead
                            if let Some(ci) = &cc.info {
                                // SAFETY: syscall
                                if unsafe { SetEvent(ci.dump_generated.raw()) } == 0 {
                                    log::error!("failed to signal dump was generated");
                                }
                            }

                            disconnect(cc.conn, &mut poll);

                            if exit {
                                return Ok(());
                            }
                        }
                        Some(ClientMsg::Tagged((Tags::RegisterRequest, register))) => {
                            let res = Self::register_client(register).and_then(|ci| {
                                let response =
                                    Self::build_register_response(&ci, &self.alive_handle)?;

                                let cc = &mut clients[pos];
                                cc.conn.pipe.write(response.as_ref())?;
                                cc.info = Some(ci);
                                Ok(())
                            });

                            if let Err(err) = res {
                                log::error!("failed to process register request: {}", err);

                                let mut cc = clients.swap_remove(pos);
                                disconnect(cc.conn, &mut poll);
                            }
                        }
                        Some(ClientMsg::Tagged((other, _data))) => {
                            log::error!(
                                "client sent an invalid message, disconnecting: {:?}",
                                other
                            );

                            let mut cc = clients.swap_remove(pos);
                            disconnect(cc.conn, &mut poll);
                        }
                        Some(ClientMsg::User((tag, buffer))) => {
                            handler.on_message(tag, buffer);

                            if let Err(e) = clients[pos].conn.pipe.write(
                                crate::Header {
                                    kind: Tags::UserTagAck as u32,
                                    size: 0,
                                }
                                .as_bytes(),
                            ) {
                                log::error!("failed to send ack: {}", e);
                            }
                        }
                        None => {
                            log::debug!("client closed socket {}", pos);
                            let mut cc = clients.swap_remove(pos);

                            disconnect(cc.conn, &mut poll);
                        }
                    }
                }
            }
        }
    }

    fn register_client(register_request: ProtoMsg) -> Result<ClientInfo, Error> {
        let rr: super::RegisterRequest = register_request.read()?;

        // We open a handle to the client process so that we can do ReadProcessMemory
        // SAFETY: syscall
        let proc_handle = unsafe {
            proc::OpenProcess(
                268435456,     /* GENERIC_ALL */
                0,             // inherit handles
                rr.process_id, // pid
            )
        };

        if proc_handle == 0 {
            return Err(last_os_error());
        }

        let process = OwnedHandle::new(proc_handle);

        // SAFETY: syscall
        let dump_generated_handle = unsafe {
            CreateEventA(
                std::ptr::null(), // Security attributes
                1,                // Manual reset
                0,                // Initial state
                std::ptr::null(), // Name
            )
        };

        if dump_generated_handle == 0 {
            return Err(last_os_error());
        }

        let dump_generated = OwnedHandle::new(dump_generated_handle);

        Ok(ClientInfo {
            process_id: rr.process_id,
            process,
            dump_generated,
        })
    }

    fn build_register_response(
        ci: &ClientInfo,
        server_alive_handle: &OwnedHandle,
    ) -> Result<ProtoMsg, Error> {
        // Duplicate the handles to the client process so that it can wait
        // on events from this process
        // SAFETY: syscall
        let current_process = unsafe { proc::GetCurrentProcess() };

        let dup_handle = |src: &OwnedHandle| -> Result<HANDLE, Error> {
            let mut target = 0;
            // SAFETY: syscall
            if unsafe {
                DuplicateHandle(
                    current_process,  // Process which owns the handle
                    src.raw(),        // handle we're duplicating
                    ci.process.raw(), // Process we're duplicating the handle to
                    &mut target,      // Duplicated handle
                    2,                // Desired access = EVENT_MODIFY_STATE
                    0,                // Inherit handle
                    0,                // Options
                )
            } == 0
            {
                Err(last_os_error())
            } else {
                Ok(target)
            }
        };

        // Duplicated handles are owned by the target process, so we have to
        // close them with DuplicateHandle as opposed to CloseHandle
        let close_dup_handle = |handle: HANDLE| {
            // SAFETY: syscall
            unsafe {
                DuplicateHandle(
                    ci.process.raw(),       // Process which owns the handle
                    handle,                 // Handle we're closing
                    0,                      // target process
                    std::ptr::null_mut(),   // target handle
                    0,                      // desired access
                    0,                      // inherit handle
                    DUPLICATE_CLOSE_SOURCE, // options, this is how we say we're closing the handle
                );
            }
        };

        let dump_generated_handle = dup_handle(&ci.dump_generated)?;
        let server_alive_handle = dup_handle(server_alive_handle).map_err(|err| {
            close_dup_handle(dump_generated_handle);
            err
        })?;

        ProtoMsg::from_pwrite(
            Tags::RegisterResponse,
            super::RegisterResponse {
                server_alive: server_alive_handle as _,
                dump_generated: dump_generated_handle as _,
            },
        )
        .map_err(|err| {
            close_dup_handle(dump_generated_handle);
            close_dup_handle(server_alive_handle);
            err
        })
    }

    fn handle_crash_request(
        client: &Client,
        crash_context: &ProtoMsg,
        handler: &dyn crate::ServerHandler,
    ) -> Result<bool, Error> {
        // We require the client info since we need the client process handle
        // to read its memory with
        let ci = client.info.as_ref().ok_or_else(|| {
            Error::from(std::io::Error::new(
                ErrorKind::NotFound,
                "client info was never received",
            ))
        })?;

        let dump_request: super::RequestDump = crash_context.read()?;

        // MiniDumpWriteDump primarily uses `EXCEPTION_POINTERS` for its crash
        // context information, but inside that is an `EXCEPTION_RECORD`, which
        // is an internally linked list, so rather than recurse and allocate until
        // the end of that linked list, we just retrieve the actual pointer from
        // the client process, and inform the dump writer that they are pointers
        // to a different process, as MiniDumpWriteDump will internally read
        // the processes memory as needed
        let exception_pointers = dump_request.exception_pointers
            as *const windows_sys::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS;
        let assertion_info = if dump_request.assertion_info != 0 {
            Some(dump_request.assertion_info as *const crash_context::RawAssertionInfo)
        } else {
            None
        };

        let cc = crash_context::CrashContext {
            exception_pointers,
            assertion_info,
            thread_id: dump_request.thread_id,
            exception_code: dump_request.exception_code,
        };

        let (mut minidump_file, minidump_path) = handler.create_minidump_file()?;

        let mut writer = minidump_writer::minidump_writer::MinidumpWriter::external_process(
            cc,
            ci.process_id,
            ci.process.raw(),
        );

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

impl Drop for Server {
    fn drop(&mut self) {
        // Signal to clients that server is shutdown in case they are waiting
        // on a minidump to be generated
        // SAFETY: syscall
        unsafe {
            proc::ReleaseMutex(self.alive_handle.raw());
        }
    }
}
