use crate::{
    Config, Error, Record, Result,
    record::{HEADER_SIZE, decode, encode},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

/// Segment file naming: zero-padded offset of first record.
fn segment_path(dir: &Path, offset: u64) -> PathBuf {
    dir.join(format!("{:020}.seg", offset))
}

/// List all segment files in the queue directory, sorted by offset.
fn list_segments(dir: &Path) -> Result<Vec<(u64, PathBuf)>> {
    let mut segments = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("seg") {
            continue;
        }

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok());

        if let Some(offset) = stem {
            segments.push((offset, path));
        }
    }

    segments.sort_by_key(|(offset, _)| *offset);
    Ok(segments)
}

/// A crash-safe, append-only disk queue.
///
/// Records are written to segment files on disk. Each successful
/// [`push`](Queue::push) is fsynced before returning, guaranteeing
/// durability.
///
/// Records are read with [`peek`](Queue::peek) and removed from
/// the queue by calling [`commit`](Queue::commit). If the process
/// crashes between peek and commit, the record will be returned
/// again on the next call to peek — this is the at-least-once
/// delivery guarantee.
pub struct Queue {
    config: Config,

    /// Current write position — byte offset in the active segment.
    write_offset: u64,

    /// Byte offset of the first byte of the next record to read,
    /// relative to the start of the current read segment.
    read_offset: u64,

    /// Index of the active write segment (its start offset).
    write_segment_offset: u64,

    /// Index of the active read segment (its start offset).
    read_segment_offset: u64,

    /// Total bytes across all unconsumed segments.
    total_bytes: u64,

    /// Number of unconsumed records.
    len: usize,

    /// File handle for the active write segment.
    write_file: File,

    /// File handle for the active read segment.
    read_file: File,
}

impl Queue {
    /// Open a queue at the given path, creating it if necessary.
    ///
    /// On first open, creates the queue directory and an initial
    /// segment file. On subsequent opens, recovers state from
    /// existing segment files.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or if
    /// any segment file is corrupt beyond recovery.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kist::{Queue, Config};
    /// use tempfile::tempdir;
    ///
    /// let dir = tempdir().unwrap();
    /// let config = Config::new(dir.path().to_path_buf());
    /// let mut queue = Queue::open(config).unwrap();
    /// ```
    pub fn open(config: Config) -> Result<Self> {
        // Create directory if it does not exist
        fs::create_dir_all(&config.storage_path).map_err(Error::DirectoryCreate)?;

        let segments = list_segments(&config.storage_path)?;

        if segments.is_empty() {
            // Fresh queue — create the first segment
            let first_path = segment_path(&config.storage_path, 0);
            let write_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&first_path)?;
            let read_file = OpenOptions::new().read(true).open(&first_path)?;

            return Ok(Self {
                config,
                write_offset: 0,
                read_offset: 0,
                write_segment_offset: 0,
                read_segment_offset: 0,
                total_bytes: 0,
                len: 0,
                write_file,
                read_file,
            });
        }

        // Recover from existing segments
        // The last segment is the active write segment
        let (write_segment_offset, write_path) = segments.last().unwrap().clone();
        let (read_segment_offset, read_path) = segments.first().unwrap().clone();

        // Recover write position — scan to end of last valid record
        let write_offset = Self::recover_write_offset(&write_path)?;

        // Count records and bytes across all segments
        let (len, total_bytes) = Self::count_records(&segments)?;

        let write_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&write_path)?;

        let read_file = OpenOptions::new().read(true).open(&read_path)?;

        Ok(Self {
            config,
            write_offset,
            read_offset: 0,
            write_segment_offset,
            read_segment_offset,
            total_bytes,
            len,
            write_file,
            read_file,
        })
    }

    /// Push a record onto the queue.
    ///
    /// The record is written to disk and fsynced before this
    /// function returns. If `Ok(())` is returned, the record
    /// will survive a process crash or power loss.
    ///
    /// # Errors
    ///
    /// - [`Error::PayloadTooLarge`] if payload exceeds `u32::MAX` bytes
    /// - [`Error::QueueFull`] if the queue has reached `max_queue_size`
    /// - [`Error::Io`] on I/O failure
    ///
    /// # Example
    ///
    /// ```rust
    /// use kist::{Queue, Config};
    /// use tempfile::tempdir;
    ///
    /// let dir = tempdir().unwrap();
    /// let mut queue = Queue::open(Config::new(dir.path().to_path_buf())).unwrap();
    /// queue.push(b"event payload").unwrap();
    /// ```
    pub fn push(&mut self, payload: &[u8]) -> Result<()> {
        let record_size = HEADER_SIZE + payload.len();

        // Check queue capacity
        if self.total_bytes + record_size as u64 > self.config.max_queue_size {
            return Err(Error::QueueFull);
        }

        let encoded = encode(payload)?;

        // Rotate segment if needed
        if self.write_offset + encoded.len() as u64 > self.config.max_segment_size
            && self.write_offset > 0
        {
            self.rotate_segment()?;
        }

        // Write to active segment
        self.write_file.write_all(&encoded)?;

        // fsync — the critical durability step
        self.write_file.sync_data()?;

        self.write_offset += encoded.len() as u64;
        self.total_bytes += encoded.len() as u64;
        self.len += 1;

        Ok(())
    }

    /// Peek at the next record without removing it from the queue.
    ///
    /// Returns `Ok(None)` if the queue is empty.
    ///
    /// The record is not removed until [`commit`](Queue::commit) is
    /// called. If the process crashes before commit, the same record
    /// will be returned on the next call to peek.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kist::{Queue, Config};
    /// use tempfile::tempdir;
    ///
    /// let dir = tempdir().unwrap();
    /// let mut queue = Queue::open(Config::new(dir.path().to_path_buf())).unwrap();
    /// queue.push(b"hello").unwrap();
    ///
    /// if let Some(record) = queue.peek().unwrap() {
    ///     println!("{:?}", record.as_bytes());
    ///     queue.commit().unwrap();
    /// }
    /// ```
    pub fn peek(&mut self) -> Result<Option<Record>> {
        if self.len == 0 {
            return Ok(None);
        }

        // Read enough bytes to decode the next record
        self.read_file.seek(SeekFrom::Start(self.read_offset))?;

        let mut buf = Vec::new();
        self.read_file.read_to_end(&mut buf)?;

        if buf.is_empty() {
            // Advance to next segment if one exists
            if let Some(next) = self.next_read_segment()? {
                self.read_segment_offset = next;
                self.read_offset = 0;
                let path = segment_path(&self.config.storage_path, self.read_segment_offset);
                self.read_file = OpenOptions::new().read(true).open(path)?;
                return self.peek();
            }
            return Ok(None);
        }

        match decode(&buf)? {
            Some((record, _)) => Ok(Some(record)),
            None => Ok(None),
        }
    }

    /// Commit the current record, advancing the read cursor.
    ///
    /// Must be called after [`peek`](Queue::peek) returns a record.
    /// Advances the read position so that the next call to peek
    /// returns the following record.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the underlying segment file cannot
    /// be read or if cursor state cannot be updated.
    pub fn commit(&mut self) -> Result<()> {
        if self.len == 0 {
            return Ok(());
        }

        // Read the record at current position to know its size
        self.read_file.seek(SeekFrom::Start(self.read_offset))?;
        let mut buf = Vec::new();
        self.read_file.read_to_end(&mut buf)?;

        if let Some((_, consumed)) = decode(&buf)? {
            self.read_offset += consumed as u64;
            self.total_bytes -= consumed as u64;
            self.len -= 1;

            // If we have consumed all records in the read segment
            // and it is not also the write segment, delete it
            if self.read_segment_offset != self.write_segment_offset {
                if let Some(next) = self.next_read_segment()? {
                    if self.read_offset >= self.segment_size(self.read_segment_offset)? {
                        let old_path =
                            segment_path(&self.config.storage_path, self.read_segment_offset);
                        fs::remove_file(old_path)?;
                        self.read_segment_offset = next;
                        self.read_offset = 0;
                        let new_path =
                            segment_path(&self.config.storage_path, self.read_segment_offset);
                        self.read_file = OpenOptions::new().read(true).open(new_path)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns the number of records currently in the queue.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the queue contains no records.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the total number of bytes used by the queue on disk.
    pub fn disk_usage(&self) -> u64 {
        self.total_bytes
    }
}

// Private implementation methods
impl Queue {
    /// Scan to the end of the last valid record in a segment file.
    /// Returns the byte offset of the end of the last valid record.
    fn recover_write_offset(path: &Path) -> Result<u64> {
        let mut file = File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let mut offset = 0usize;

        loop {
            match decode(&buf[offset..]) {
                Ok(Some((_, consumed))) => {
                    offset += consumed;
                }
                Ok(None) => break,
                Err(_) => break, // Partial write — stop here
            }
        }

        Ok(offset as u64)
    }

    /// Count records and total bytes across a list of segments.
    fn count_records(segments: &[(u64, PathBuf)]) -> Result<(usize, u64)> {
        let mut total_records = 0;
        let mut total_bytes = 0u64;

        for (_, path) in segments {
            let mut file = File::open(path)?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;

            let mut offset = 0;
            loop {
                match decode(&buf[offset..]) {
                    Ok(Some((_, consumed))) => {
                        total_records += 1;
                        total_bytes += consumed as u64;
                        offset += consumed;
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        }

        Ok((total_records, total_bytes))
    }

    /// Create a new segment file and make it the active write segment.
    fn rotate_segment(&mut self) -> Result<()> {
        let new_offset = self.write_segment_offset + self.write_offset;
        let new_path = segment_path(&self.config.storage_path, new_offset);

        self.write_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_path)?;

        self.write_segment_offset = new_offset;
        self.write_offset = 0;

        Ok(())
    }

    /// Find the offset of the next segment after the current read segment.
    fn next_read_segment(&self) -> Result<Option<u64>> {
        let segments = list_segments(&self.config.storage_path)?;
        let mut found = false;

        for (offset, _) in &segments {
            if found {
                return Ok(Some(*offset));
            }
            if *offset == self.read_segment_offset {
                found = true;
            }
        }

        Ok(None)
    }

    /// Get the size in bytes of a specific segment.
    fn segment_size(&self, segment_offset: u64) -> Result<u64> {
        let path = segment_path(&self.config.storage_path, segment_offset);
        Ok(fs::metadata(path)?.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn open_queue() -> (Queue, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let config = Config::new(dir.path().to_path_buf());
        let queue = Queue::open(config).unwrap();
        (queue, dir)
    }

    #[test]
    fn push_and_peek() {
        let (mut queue, _dir) = open_queue();
        queue.push(b"hello").unwrap();
        let record = queue.peek().unwrap().unwrap();
        assert_eq!(record.as_bytes(), b"hello");
    }

    #[test]
    fn push_peek_commit() {
        let (mut queue, _dir) = open_queue();
        queue.push(b"hello").unwrap();
        queue.push(b"world").unwrap();

        let r1 = queue.peek().unwrap().unwrap();
        assert_eq!(r1.as_bytes(), b"hello");
        queue.commit().unwrap();

        let r2 = queue.peek().unwrap().unwrap();
        assert_eq!(r2.as_bytes(), b"world");
        queue.commit().unwrap();

        assert!(queue.peek().unwrap().is_none());
        assert!(queue.is_empty());
    }

    #[test]
    fn empty_queue_peek_returns_none() {
        let (mut queue, _dir) = open_queue();
        assert!(queue.peek().unwrap().is_none());
    }

    #[test]
    fn len_tracks_correctly() {
        let (mut queue, _dir) = open_queue();
        assert_eq!(queue.len(), 0);

        queue.push(b"a").unwrap();
        assert_eq!(queue.len(), 1);

        queue.push(b"b").unwrap();
        assert_eq!(queue.len(), 2);

        queue.peek().unwrap().unwrap();
        queue.commit().unwrap();
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn fifo_ordering() {
        let (mut queue, _dir) = open_queue();
        let events = [b"first".as_slice(), b"second", b"third"];

        for event in &events {
            queue.push(event).unwrap();
        }

        for expected in &events {
            let record = queue.peek().unwrap().unwrap();
            assert_eq!(record.as_bytes(), *expected);
            queue.commit().unwrap();
        }
    }

    #[test]
    fn survives_reopen() {
        let dir = tempdir().unwrap();

        {
            let config = Config::new(dir.path().to_path_buf());
            let mut queue = Queue::open(config).unwrap();
            queue.push(b"persisted").unwrap();
        }

        // Reopen — simulates process restart
        {
            let config = Config::new(dir.path().to_path_buf());
            let mut queue = Queue::open(config).unwrap();
            let record = queue.peek().unwrap().unwrap();
            assert_eq!(record.as_bytes(), b"persisted");
        }
    }

    #[test]
    fn queue_full_returns_error() {
        let dir = tempdir().unwrap();
        let config = Config::new(dir.path().to_path_buf()).max_queue_size(100); // tiny limit
        let mut queue = Queue::open(config).unwrap();

        // Push until full
        let mut hit_full = false;
        for _ in 0..100 {
            match queue.push(b"payload") {
                Ok(_) => {}
                Err(Error::QueueFull) => {
                    hit_full = true;
                    break;
                }
                Err(e) => panic!("unexpected error: {}", e),
            }
        }
        assert!(hit_full);
    }

    #[test]
    fn segment_rotation() {
        let dir = tempdir().unwrap();
        // Tiny segment size forces rotation
        let config = Config::new(dir.path().to_path_buf())
            .max_segment_size(64)
            .max_queue_size(64 * 1024);
        let mut queue = Queue::open(config).unwrap();

        // Push enough to force multiple segments
        for i in 0..20u32 {
            queue.push(format!("event-{}", i).as_bytes()).unwrap();
        }

        let segments = list_segments(dir.path()).unwrap();
        assert!(segments.len() > 1, "expected multiple segments");

        // All records still readable in order
        for i in 0..20u32 {
            let record = queue.peek().unwrap().unwrap();
            assert_eq!(record.as_bytes(), format!("event-{}", i).as_bytes());
            queue.commit().unwrap();
        }
    }
}
