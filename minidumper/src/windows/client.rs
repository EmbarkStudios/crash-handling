use super::{last_os_error, Message, OwnedHandle, Tags};
use crate::{write_stderr, Error};
use mio::windows::NamedPipe;
use parking_lot::Mutex;
use std::{
    fs::File,
    io::{ErrorKind, Write},
    os::windows::io::AsRawHandle,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_PIPE_BUSY, HANDLE},
    Storage::FileSystem as fs,
    System::{
        Pipes as pipe,
        Threading::{ResetEvent, SetEvent, WaitForMultipleObjects, WAIT_OBJECT_0},
    },
};

const SERVER_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(15000);

pub struct Client {
    /// The pipe handle. Note that we don't use mio's `NamedPipe` here since the
    /// client side is fairly simple and importantly uses eg. `TransactNamedPipe`
    /// which might interfere with mio
    pipe: Mutex<File>,
    /// Handle to an event that will become signaled with `WAIT_ABANDONED`
    /// if the server process goes down.
    server_alive: OwnedHandle,
    /// Handle to an event that will be signaled when the server has finished
    /// generating a minidump
    dump_generated: OwnedHandle,
}

impl Client {
    /// Creates a new client with the given name.
    pub fn with_name(name: impl AsRef<str>) -> Result<Self, Error> {
        let pipe_name = format!("\\\\.\\pipe\\{}\0", name.as_ref());

        let mut pipe = Self::connect(&pipe_name)?;
        let register_response = Self::register(&mut pipe)?;

        Ok(Self {
            pipe: Mutex::new(pipe),
            server_alive: OwnedHandle::new(register_response.server_alive as HANDLE),
            dump_generated: OwnedHandle::new(register_response.dump_generated as HANDLE),
        })
    }

    /// Requests that the server generate a minidump for the specified crash
    /// context.
    ///
    /// This blocks until the server has finished writing the minidump.
    pub fn request_dump(
        &self,
        crash_context: &crash_context::CrashContext,
        debug_print: bool,
    ) -> Result<(), Error> {
        // Ensure the event is not signaled, the server will signal it when
        // it has finished generating the minidump
        // SAFETY: syscall
        if unsafe { ResetEvent(self.dump_generated.raw()) } == 0 {
            return Err(last_os_error());
        }

        // We're passing the pointer value to the server process
        let assertion_info = crash_context
            .assertion_info
            .as_ref()
            .map_or(0, |ai| ai as *const _ as super::ProtoPointer);
        // SAFETY: checking validity before dereferencing
        let exception_code = unsafe {
            if !crash_context.exception_pointers.is_null()
                && !(*crash_context.exception_pointers)
                    .ExceptionRecord
                    .is_null()
            {
                (*(*crash_context.exception_pointers).ExceptionRecord).ExceptionCode
            } else {
                0
            }
        };

        /// Inform the server that we wish it to generate a minidump
        let request_msg = Message::<{ 8 + 24 }>::from_pwrite(
            Tags::RequestDump,
            super::RequestDump {
                exception_pointers: crash_context.exception_pointers as _,
                assertion_info,
                thread_id: crash_context.thread_id,
                exception_code,
            },
        )?;

        {
            let mut pipe = self.pipe.lock();
            pipe.write_all(request_msg.as_ref())?;
        }

        let wait_handles = [self.dump_generated.raw(), self.server_alive.raw()];

        // Wait for _either_ the crash dump to be created _or_ the server to die
        // in case a crash occurs in the server process itself
        // SAFETY: syscall
        if unsafe {
            WaitForMultipleObjects(
                wait_handles.len() as u32,
                wait_handles.as_ptr(),
                0,
                SERVER_TIMEOUT.as_millis() as u32,
            )
        } == WAIT_OBJECT_0
        {
            Ok(())
        } else {
            // Don't use std::io::Error::new since it allocates
            Err(std::io::Error::from(ErrorKind::TimedOut).into())
        }
    }

    /// Sends a message to the server.
    #[inline]
    pub fn send_message<const N: usize>(&self, message: &Message<N>) -> Result<(), Error> {
        let mut lock = self.pipe.lock();
        Self::transact(&mut lock, message.as_ref()).map(|_res| ())
    }

    /// Attempts to connect to a named pipe server with the given name
    fn connect(pipe_name: &str) -> Result<File, Error> {
        use std::os::windows::fs::OpenOptionsExt;

        const ACCESS_MODE: u32 =
            fs::FILE_READ_DATA | fs::FILE_WRITE_DATA | fs::FILE_WRITE_ATTRIBUTES;
        const ATTRIBUTES: u32 = fs::SECURITY_IDENTIFICATION | fs::SECURITY_SQOS_PRESENT;

        let mut opts = std::fs::OpenOptions::new();
        opts.access_mode(ACCESS_MODE).security_qos_flags(ATTRIBUTES);

        for _ in 0..2 {
            match opts.open(pipe_name) {
                Ok(pipe) => {
                    // Using the \\.\pipe syntax automatically puts the pipe
                    // into byte mode, but we want to use message mode
                    // SAFETY: syscall
                    if unsafe {
                        pipe::SetNamedPipeHandleState(
                            pipe.as_raw_handle() as HANDLE,
                            &pipe::PIPE_READMODE_MESSAGE,
                            std::ptr::null(),
                            std::ptr::null(),
                        )
                    } == 0
                    {
                        return Err(last_os_error());
                    } else {
                        return Ok(pipe);
                    }
                }
                Err(e) => {
                    if let Some(os_err) = e.raw_os_error() {
                        // This is the only error that we can retry
                        if os_err != ERROR_PIPE_BUSY as i32 {
                            return Err(e.into());
                        }
                    }
                }
            }

            // Cannot continue retrying if wait on pipe fails.
            // SAFETY: syscall
            if unsafe {
                pipe::WaitNamedPipeA(pipe_name.as_ptr(), 2000 /* milliseconds */)
            } == 0
            {
                break;
            }
        }

        Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "unable to connect to pipe").into())
    }

    fn register(pipe: &mut File) -> Result<super::RegisterResponse, Error> {
        let message = Message::<12>::from_pwrite(
            Tags::RegisterRequest,
            super::RegisterRequest {
                process_id: std::process::id(),
            },
        )?;

        let response = Self::transact(pipe, message.as_ref())?;

        if response.kind() != Tags::RegisterResponse as u32 {
            return Err(Error::Protocol {
                expected: Tags::RegisterResponse as u32,
                received: response.kind(),
            });
        }

        response.read()
    }

    /// Sends a payload to the server and receives a response
    fn transact(pipe: &mut File, buffer: &[u8]) -> Result<Message<256>, Error> {
        let buffer_len = buffer.len() as u32;

        // The maximum guaranteed size of a named pipe transaction is 64 kilobytes.
        // In some limited cases, transactions beyond 64 kilobytes are possible,
        // depending on OS versions participating in the transaction and dynamic
        // network conditions. However, there is no guarantee that transactions
        // above 64 kilobytes will succeed. Therefore it's recommended that
        // named pipe transactions be limited to 64 kilobytes of data.
        if buffer_len > 64 * 1024 {
            return Err(scroll::Error::TooBig {
                size: buffer_len as usize,
                len: 64 * 1024,
            }
            .into());
        }

        let mut msg = Message::<256>::default();
        let msg_len = msg.as_ref().len() as u32;
        let mut read = 0;
        // SAFETY: syscall
        if unsafe {
            pipe::TransactNamedPipe(
                pipe.as_raw_handle() as HANDLE,
                buffer.as_ptr().cast(),
                buffer_len,
                msg.as_mut().as_mut_ptr().cast(),
                msg_len,
                &mut read,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(last_os_error());
        }

        msg.validate(read)?;
        Ok(msg)
    }
}

impl Drop for Client {
    fn drop(&mut self) {}
}
