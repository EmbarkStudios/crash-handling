#![allow(unsafe_code)] // we do a lot of syscalls

mod client;
mod server;
mod uds;

pub use client::Client;
pub use server::Server;

// This will of course break if the client and server are built for different
// arches, but that is the fault of the user in that case
cfg_if::cfg_if! {
    if #[cfg(target_pointer_width = "32")] {
        type ProtoPointer = u32;
    } else if #[cfg(target_pointer_width = "64")] {
        type ProtoPointer = u64;
    }
}

#[derive(scroll::Pwrite, scroll::Pread, scroll::SizeWith)]
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
