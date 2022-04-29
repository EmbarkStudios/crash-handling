// crate-specific exceptions:

mod errors;

pub use errors::Error;
use std::{fs::File, path::PathBuf};

mod ipc;
pub use ipc::{Client, Server};

/// The result of a successful minidump generation.
pub struct MinidumpBinary {
    /// The file the minidump was written to, as provided by [`ServerHandler::create_minidump_file`]
    pub file: File,
    /// The path to the file as provided by [`ServerHandler::create_minidump_file`].
    pub path: PathBuf,
    /// The in-memory contents of the minidump, may be empty
    pub contents: Vec<u8>,
}

/// Allows user code to hook into the server to avoid hardcoding too many details
pub trait ServerHandler: Send + Sync {
    /// Called when a crash request has been received and a backing file needs
    /// to be created to store it.
    fn create_minidump_file(&self) -> Result<(File, PathBuf), std::io::Error>;
    /// Called when a crash has been fully written as a minidump to the provided
    /// file. Also returns the full heap buffer as well.
    ///
    /// A return value of true indicates that the message loop should exit and
    /// stop processing messages.
    fn on_minidump_created(&self, result: Result<MinidumpBinary, Error>) -> bool;
    /// Called when the client sends a user message sent from the client with
    /// `send_message`
    fn on_message(&self, kind: u32, buffer: Vec<u8>);
    /// Optional allocation function for the buffer used to store a message.
    ///
    /// Defaults to creating a new vec.
    fn message_alloc(&self) -> Vec<u8> {
        Vec::new()
    }
}
