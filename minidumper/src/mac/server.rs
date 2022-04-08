use crate::Error;

pub struct Server {
    //listener: UnixSeqpacketListener,
    port_name: std::ffi::CString,
    port: mp::mach_port_t,
}

impl Server {
    pub fn with_name(name: impl AsRef<str>) -> Result<Self, Error> {
        Ok(Self { port_name, port })
    }

    /// Runs the server loop, accepting client connections and requests to
    /// create minidumps
    pub fn run(
        &self,
        handler: Box<dyn crate::ServerHandler>,
        shutdown: &std::sync::atomic::AtomicBool,
    ) -> Result<(), Error> {
    }
}
