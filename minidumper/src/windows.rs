#![allow(unsafe_code)] // we do a lot of syscalls

mod client;
mod server;

pub use client::Client;
pub use server::Server;

use crate::errors::Error;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};

// HANDLE is an isize which scroll doesn't implement anything for since it is
// architecture agnostic so size them explicitly. This will of course break
// if the client and server are built for different arches, but that is the fault
// of the user in that case
cfg_if::cfg_if! {
    if #[cfg(target_pointer_width = "32")] {
        type ProtoHandle = i32;
        type ProtoPointer = u32;
    } else if #[cfg(target_pointer_width = "64")] {
        type ProtoHandle = i64;
        type ProtoPointer = u64;
    }
}

/// Wrapper for HANDLE so we can easily close it on drop
struct OwnedHandle(HANDLE);

impl OwnedHandle {
    #[inline]
    fn new(h: HANDLE) -> Self {
        Self(h)
    }

    #[inline]
    fn into_raw(self) -> HANDLE {
        let h = self.0;
        // Critical so that drop isn't called
        std::mem::forget(self);
        h
    }

    #[inline]
    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: syscall
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(u32)]
pub(crate) enum Tags {
    /// Request a crash dump
    RequestDump = 0,
    /// Client requesting registration with server
    RegisterRequest = 1,
    /// Server response to client registration request
    RegisterResponse = 2,
    /// Server acknolwedgement of a client user request
    UserTagAck = 3,
    /// Client user request (offset)
    UserRequest = 4,
}

impl Tags {
    pub(crate) fn from_u32(tag: u32) -> Result<Self, u32> {
        Ok(match tag {
            0 => Self::RequestDump,
            1 => Self::RegisterRequest,
            2 => Self::RegisterResponse,
            3 => Self::UserTagAck,
            _ => return Err(tag),
        })
    }
}

use scroll::{ctx, Pread, Pwrite, SizeWith};

#[derive(Pwrite, Pread, SizeWith)]
struct RequestDump {
    /// The address of an `EXCEPTION_POINTERS` in the client's memory
    exception_pointers: ProtoPointer,
    /// The id of the thread in the client process in which the crash originated
    thread_id: u32,
    /// The top level exception code, also found in the `EXCEPTION_POINTERS.ExceptionRecord.ExceptionCode`
    exception_code: i32,
}

#[derive(Pwrite, Pread, SizeWith)]
struct RegisterRequest {
    /// Process id for the client process that is requesting registration with
    /// the server
    process_id: u32,
}

#[derive(Pwrite, Pread, SizeWith)]
struct RegisterResponse {
    /// Event handle used to indicate the server process is alive
    server_alive: ProtoHandle,
    /// Event handle used to indicate the minidump has finished being generated
    dump_generated: ProtoHandle,
}

/// A single encoded message that is sent to the server.
///
/// Due to Windows not having vectored reads/writes it's better to keep both
/// the header and the payload in a single blob. Note that the message size should
/// account for the 8 byte header present at the beginning of the message
pub struct Message<const N: usize> {
    buffer: [u8; N],
    len: usize,
}

impl<const N: usize> Message<N> {
    /// Creates a message, but will fail if the specified buffer is too large
    /// to fit
    pub fn from_buffer(kind: u32, data: impl AsRef<[u8]>) -> Option<Self> {
        let mut s = Self::default();

        if s.set_buffer(data) {
            s.set_kind(kind + Tags::UserRequest as u32);
            Some(s)
        } else {
            None
        }
    }

    pub(crate) fn from_pwrite<M>(kind: Tags, data: M) -> Result<Self, Error>
    where
        M: ctx::TryIntoCtx<scroll::Endian, Error = scroll::Error> + ctx::SizeWith<scroll::Endian>,
    {
        let mut s = Self::default();

        let size = M::size_with(&scroll::Endian::Little);
        if size + 8 > N {
            return Err(scroll::Error::TooBig { size, len: N - 8 }.into());
        }

        let written = s.buffer.pwrite(data, 8)?;
        s.set_kind(kind as u32);
        s.set_len(written as u32);

        Ok(s)
    }

    /// Overwrites the current buffer
    pub fn set_buffer(&mut self, data: impl AsRef<[u8]>) -> bool {
        let buf = data.as_ref();

        if buf.len() + 8 > N {
            return false;
        }

        self.buffer[8..8 + buf.len()].copy_from_slice(buf);
        true
    }

    #[inline]
    pub(crate) fn kind(&self) -> u32 {
        self.buffer
            .pread(0)
            .expect("impossible if mistakes weren't made :p")
    }

    #[inline]
    fn set_kind(&mut self, kind: u32) {
        self.buffer
            .pwrite(kind, 0)
            .expect("impossible if mistakes weren't made :p");
    }

    #[inline]
    fn set_len(&mut self, len: u32) {
        self.len = len as usize + 8;
        self.buffer
            .pwrite(len, 4)
            .expect("impossible if mistakes weren't made :p");
    }

    /// Validates the buffer after a read
    pub(crate) fn validate(&mut self, read_len: u32) -> Result<(), Error> {
        let buffer_len: u32 = self
            .buffer
            .pread(4)
            .map_err(|_e| std::io::Error::from(std::io::ErrorKind::InvalidData))?;

        if read_len != buffer_len + 8 {
            Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into())
        } else {
            self.len = read_len as usize;
            Ok(())
        }
    }

    #[inline]
    pub(crate) fn read<'a, R>(&'a self) -> Result<R, Error>
    where
        R: ctx::TryFromCtx<'a, scroll::Endian, Error = scroll::Error>,
    {
        self.buffer
            .pread(8)
            .map_err(|_e| std::io::Error::from(std::io::ErrorKind::InvalidData).into())
    }
}

impl<const N: usize> AsRef<[u8]> for Message<N> {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[..self.len]
    }
}

impl<const N: usize> AsMut<[u8]> for Message<N> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }
}

impl<const N: usize> Default for Message<N> {
    fn default() -> Self {
        Self {
            buffer: [0u8; N],
            len: 0,
        }
    }
}

#[inline]
pub(crate) fn last_os_error() -> Error {
    std::io::Error::last_os_error().into()
}
