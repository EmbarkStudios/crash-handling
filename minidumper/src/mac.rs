mod client;
mod server;
mod uds;

pub use client::Client;
pub use server::Server;

#[derive(scroll::Pwrite, scroll::Pread, scroll::SizeWith)]
struct DumpRequest {
    /// The exception code
    code: i64,
    /// Optional subcode, typically only present for `EXC_BAD_ACCESS` exceptions
    subcode: i64,
    /// The process which crashed
    task: u32,
    /// The thread in the process that crashed
    thread: u32,
    /// The thread that handled the exception. This may be useful to ignore.
    handler_thread: u32,
    /// The exception kind
    kind: i32,
    /// Boolean to indicate if there is exception information or not
    has_exception: u8,
    /// Boolean to indicate if there is a subcode
    has_subcode: u8,
}
