use std::io;

/// Errors returned by Kist operations.
#[derive(Debug)]
pub enum Error {
    /// An I/O error occurred reading or writing segment files.
    Io(io::Error),

    /// The payload exceeds the maximum allowed size.
    /// Maximum payload size is [`u32::MAX`] bytes.
    PayloadTooLarge,

    /// The queue has reached its maximum configured size.
    /// No more records can be pushed until some are committed.
    QueueFull,

    /// A record on disk failed its checksum verification.
    /// This indicates data corruption.
    ChecksumMismatch {
        /// Expected CRC32C checksum stored in the record header.
        expected: u32,
        /// Actual CRC32C checksum computed from the payload.
        actual: u32,
    },

    /// A segment file contains an invalid or unrecognised magic
    /// byte sequence. The file may be corrupt or not a Kist segment.
    InvalidMagic,

    /// The queue directory could not be created.
    DirectoryCreate(io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {}", e),
            Error::PayloadTooLarge => {
                write!(f, "payload exceeds maximum size of {} bytes", u32::MAX)
            }
            Error::QueueFull => write!(f, "queue has reached its maximum configured size"),
            Error::ChecksumMismatch { expected, actual } => write!(
                f,
                "checksum mismatch: expected {:#010x}, got {:#010x}",
                expected, actual
            ),
            Error::InvalidMagic => write!(f, "invalid magic bytes — file may be corrupt"),
            Error::DirectoryCreate(e) => write!(f, "failed to create queue directory: {}", e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::DirectoryCreate(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
