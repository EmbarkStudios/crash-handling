#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("the socket name is invalid")]
    InvalidName,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("client process requesting crash dump has an unknown or invalid pid")]
    UnknownClientPid,
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[error(transparent)]
    Writer(#[from] minidump_writer::errors::WriterError),
    #[cfg(target_os = "windows")]
    #[error(transparent)]
    Writer(#[from] minidump_writer::errors::Error),
    #[cfg(target_os = "macos")]
    #[error(transparent)]
    Writer(#[from] minidump_writer::errors::WriterError),
    #[cfg(target_os = "windows")]
    #[error("protocol error, expected tag '{expected}' but received '{received}'")]
    Protocol { expected: u32, received: u32 },
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[error(transparent)]
    Scroll(#[from] scroll::Error),
}
