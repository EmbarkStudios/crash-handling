use std::fmt;

#[derive(Debug)]
pub enum Error {
    OutOfMemory,
    InvalidArgs,
    Format(std::fmt::Error),
    /// For simplicity sake, only one `ExceptionHandler` can be registered
    /// at any one time.
    HandlerAlreadyInstalled,
    Io(std::io::Error),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Format(inner) => Some(inner),
            Self::Io(inner) => Some(inner),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfMemory => f.write_str("unable to allocate memory"),
            Self::InvalidArgs => f.write_str("invalid arguments provided"),
            Self::Format(e) => write!(f, "{}", e),
            Self::HandlerAlreadyInstalled => {
                f.write_str("an exception handler is already installed")
            }
            Self::Io(e) => write!(f, "{}", e),
        }
    }
}

impl From<std::fmt::Error> for Error {
    fn from(e: std::fmt::Error) -> Self {
        Self::Format(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
