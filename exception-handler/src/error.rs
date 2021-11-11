use std::fmt;

#[derive(Debug)]
pub enum Error {
    OutOfMemory,
    InvalidArgs,
    Format(std::fmt::Error),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Format(inner) => Some(inner),
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
        }
    }
}

impl From<std::fmt::Error> for Error {
    fn from(e: std::fmt::Error) -> Self {
        Self::Format(e)
    }
}
