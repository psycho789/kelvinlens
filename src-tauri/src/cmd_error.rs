//! The error every command rejects with: `{ kind, message }` on the wire (`CmdError` in `src/api.ts`).

use std::sync::PoisonError;

use kelvinlens_core::Error as CoreError;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    InvalidFile,
    InvalidInput,
    Unsupported,
    NotFound,
    Io,
    Internal,
}

#[derive(Debug, Serialize)]
pub struct CmdError {
    pub(crate) kind: ErrorKind,
    pub(crate) message: String,
}

impl CmdError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        CmdError { kind, message: message.into() }
    }
    pub fn invalid_file(m: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidFile, m)
    }
    pub fn invalid_input(m: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidInput, m)
    }
    pub fn unsupported(m: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, m)
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, m)
    }
    pub fn internal(m: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, m)
    }
}

impl From<CoreError> for CmdError {
    fn from(e: CoreError) -> Self {
        let kind = match &e {
            CoreError::Open(_) => ErrorKind::InvalidFile,
            CoreError::Io(_) => ErrorKind::Io,
            CoreError::Params(_) | CoreError::Panorama(_) => ErrorKind::InvalidInput,
            CoreError::Encode(_) | CoreError::Report(_) => ErrorKind::Internal,
        };
        CmdError::new(kind, e.to_string())
    }
}

impl From<std::io::Error> for CmdError {
    fn from(e: std::io::Error) -> Self {
        CoreError::Io(e).into()
    }
}

impl<T> From<PoisonError<T>> for CmdError {
    fn from(_: PoisonError<T>) -> Self {
        CmdError::internal("An earlier operation failed and left the open images unusable; restart Kelvinlens")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn kind_and_text(e: CoreError) -> (ErrorKind, String) {
        let c = CmdError::from(e);
        (c.kind, c.message)
    }

    #[test]
    fn core_errors_map_to_kinds_and_keep_their_text() {
        assert_eq!(
            kind_and_text(CoreError::Open("x".into())),
            (ErrorKind::InvalidFile, "Could not open thermogram: x".into())
        );
        assert_eq!(kind_and_text(CoreError::Params("bad".into())), (ErrorKind::InvalidInput, "bad".into()));
        assert_eq!(kind_and_text(CoreError::Panorama("far".into())), (ErrorKind::InvalidInput, "far".into()));
        assert_eq!(
            kind_and_text(CoreError::Encode("e".into())),
            (ErrorKind::Internal, "Could not encode image: e".into())
        );
        assert_eq!(
            kind_and_text(CoreError::Report("r".into())),
            (ErrorKind::Internal, "Could not create report: r".into())
        );
        let io = CmdError::from(std::io::Error::other("gone"));
        assert_eq!((io.kind, io.message.as_str()), (ErrorKind::Io, "File error: gone"));
    }

    #[test]
    fn constructors_set_kind_and_message() {
        let cases = [
            (CmdError::invalid_file("a"), ErrorKind::InvalidFile),
            (CmdError::invalid_input("a"), ErrorKind::InvalidInput),
            (CmdError::unsupported("a"), ErrorKind::Unsupported),
            (CmdError::not_found("a"), ErrorKind::NotFound),
            (CmdError::internal("a"), ErrorKind::Internal),
        ];
        for (e, kind) in cases {
            assert_eq!((e.kind, e.message.as_str()), (kind, "a"));
        }
    }

    #[test]
    fn a_poisoned_lock_is_an_internal_error() {
        let lock = Mutex::new(0);
        std::thread::scope(|s| {
            let _ = s
                .spawn(|| {
                    let _guard = lock.lock().unwrap();
                    panic!("poison it");
                })
                .join();
        });
        let e = CmdError::from(lock.lock().unwrap_err());
        assert_eq!(e.kind, ErrorKind::Internal);
    }
}
