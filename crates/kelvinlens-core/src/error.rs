use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// The file could not be read or is not a supported thermogram.
    Open(String),
    Io(std::io::Error),
    /// A capture parameter is out of range, or the format cannot be recomputed.
    Params(String),
    Encode(String),
    Report(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Open(m) => write!(f, "Could not open thermogram: {m}"),
            Error::Io(e) => write!(f, "File error: {e}"),
            Error::Params(m) => write!(f, "{m}"),
            Error::Encode(m) => write!(f, "Could not encode image: {m}"),
            Error::Report(m) => write!(f, "Could not create report: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
