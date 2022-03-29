#![allow(unsafe_code)] // we do a lot of syscalls

mod client;
mod server;
mod uds;

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
    /// Client user request (offset)
    UserRequest = 2,
}

impl Tags {
    pub(crate) fn from_u32(tag: u32) -> Result<Self, u32> {
        Ok(match tag {
            0 => Self::RequestDump,
            1 => Self::RegisterRequest,
            _ => return Err(tag),
        })
    }
}

use scroll::{ctx, Pread, Pwrite, SizeWith};

#[derive(Pwrite, Pread, SizeWith)]
struct DumpRequest {
    /// The address of an `EXCEPTION_POINTERS` in the client's memory
    exception_pointers: ProtoPointer,
    /// The process id of the client process
    process_id: u32,
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

#[inline]
pub(crate) fn last_os_error() -> Error {
    std::io::Error::last_os_error().into()
}

pub fn make_temp_path(filename: impl AsRef<std::ffi::OsStr>) -> Result<std::path::PathBuf, Error> {
    use std::os::windows::ffi::OsStringExt;

    // TODO: as noted in the documentation for this function, this doesn't actually
    // check that the path exists, so a better method would be to do the same
    // order of checks in the documentation until we find a directory that does
    // exist, but I'm too lazy right now, as, in practice, these directories
    // will almost always exist already
    let mut max_path = [0u16; 261];
    // SAFETY: syscall
    let len = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetTempPathW(
            max_path.len() as u32,
            max_path.as_mut_ptr(),
        )
    };

    if len == 0 {
        Err(last_os_error())
    } else {
        let mut pb =
            std::path::PathBuf::from(std::ffi::OsString::from_wide(&max_path[..len as usize]));
        pb.set_file_name(filename);
        Ok(pb)
    }
}
