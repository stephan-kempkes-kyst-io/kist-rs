//! # Kist
//!
//! A crash-safe, append-only disk queue for Rust.
//!
//! Kist guarantees that a [`push`](Queue::push) which returns `Ok(())`
//! will survive process crashes, power loss, and OS restarts.
//! Events are stored in the order they are pushed and read in
//! the same order (FIFO).
//!
//! ## Quick Start
//!
//! ```rust
//! use kist::{Queue, Config};
//! use std::path::PathBuf;
//! use tempfile::tempdir;
//!
//! let dir = tempdir().unwrap();
//! let config = Config::new(dir.path().to_path_buf());
//! let mut queue = Queue::open(config).unwrap();
//!
//! // Write — durable after this returns
//! queue.push(b"hello world").unwrap();
//!
//! // Read
//! if let Some(record) = queue.peek().unwrap() {
//!     println!("got: {:?}", record.as_bytes());
//!     queue.commit().unwrap();
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]

mod config;
mod error;
mod queue;
mod record;

pub use config::Config;
pub use error::Error;
pub use queue::Queue;
pub use record::Record;

/// Result type for all Kist operations.
pub type Result<T> = std::result::Result<T, Error>;
