use std::path::PathBuf;

/// Configuration for a [`Queue`](crate::Queue).
#[derive(Debug, Clone)]
pub struct Config {
    /// Directory where segment files are stored.
    /// Will be created if it does not exist.
    pub(crate) storage_path: PathBuf,

    /// Maximum size of a single segment file in bytes.
    /// When the active segment exceeds this size, a new
    /// segment is created.
    ///
    /// Default: 64MB
    pub(crate) max_segment_size: u64,

    /// Maximum total size of all segment files in bytes.
    /// Push will return [`Error::QueueFull`](crate::Error) when
    /// this limit is reached.
    ///
    /// Default: 512MB
    pub(crate) max_queue_size: u64,
}

impl Config {
    /// Create a new configuration with default values.
    ///
    /// # Arguments
    ///
    /// * `storage_path` — directory where segment files will be stored
    ///
    /// # Example
    ///
    /// ```rust
    /// use kist::Config;
    /// use std::path::PathBuf;
    ///
    /// let config = Config::new(PathBuf::from("/var/lib/myapp/queue"));
    /// ```
    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            storage_path,
            max_segment_size: 64 * 1024 * 1024, // 64MB
            max_queue_size: 512 * 1024 * 1024,  // 512MB
        }
    }

    /// Set the maximum segment file size in bytes.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kist::Config;
    /// use std::path::PathBuf;
    ///
    /// let config = Config::new(PathBuf::from("/tmp/queue"))
    ///     .max_segment_size(16 * 1024 * 1024); // 16MB
    /// ```
    pub fn max_segment_size(mut self, bytes: u64) -> Self {
        self.max_segment_size = bytes;
        self
    }

    /// Set the maximum total queue size in bytes.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kist::Config;
    /// use std::path::PathBuf;
    ///
    /// let config = Config::new(PathBuf::from("/tmp/queue"))
    ///     .max_queue_size(1024 * 1024 * 1024); // 1GB
    /// ```
    pub fn max_queue_size(mut self, bytes: u64) -> Self {
        self.max_queue_size = bytes;
        self
    }
}
