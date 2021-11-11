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
    Writer(#[from] minidump_writer_linux::errors::WriterError),
}
