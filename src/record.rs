use crate::{Error, Result};
use crc32fast::Hasher;

/// Magic bytes written at the start and end of every record.
/// Used to detect truncated or corrupted records on recovery.
pub(crate) const MAGIC: [u8; 4] = [0x4B, 0x49, 0x53, 0x54]; // "KIST"

/// Size of the record header in bytes.
///
/// Layout:
/// ```text
/// [magic: 4][length: 4][checksum: 4] = 12 bytes header
/// [payload: N bytes]
/// ```
pub(crate) const HEADER_SIZE: usize = MAGIC.len() + 8;

/// A record read from the queue.
///
/// Obtained by calling [`Queue::peek`](crate::Queue::peek).
/// The record remains in the queue until [`Queue::commit`](crate::Queue::commit)
/// is called — this is the at-least-once delivery guarantee.
#[derive(Debug, Clone)]
pub struct Record {
    payload: Vec<u8>,
}

impl Record {
    /// Returns the raw payload bytes of this record.
    pub fn as_bytes(&self) -> &[u8] {
        &self.payload
    }

    /// Consumes the record and returns the payload as a Vec.
    pub fn into_bytes(self) -> Vec<u8> {
        self.payload
    }

    /// Returns the length of the payload in bytes.
    pub fn len(&self) -> usize {
        self.payload.len()
    }

    /// Returns true if the payload is empty.
    pub fn is_empty(&self) -> bool {
        self.payload.is_empty()
    }
}

/// Encode a payload into a complete record with header and trailer.
///
/// Record format:
/// ```text
/// [MAGIC: 4 bytes]
/// [length: 4 bytes, little-endian u32]
/// [checksum: 4 bytes, little-endian u32, CRC32C of payload]
/// [payload: N bytes]
/// ```
pub(crate) fn encode(payload: &[u8]) -> Result<Vec<u8>> {
    let length = u32::try_from(payload.len()).map_err(|_| Error::PayloadTooLarge)?;

    let checksum = compute_checksum(payload);

    let mut buf = Vec::with_capacity(HEADER_SIZE + payload.len());

    // Header
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&length.to_le_bytes());
    buf.extend_from_slice(&checksum.to_le_bytes());

    // Payload
    buf.extend_from_slice(payload);

    Ok(buf)
}

/// Attempt to decode a record from the start of `buf`.
///
/// Returns `Ok(Some((record, bytes_consumed)))` on success.
/// Returns `Ok(None)` if `buf` does not contain a complete record.
/// Returns `Err` if the record is present but corrupt.
pub(crate) fn decode(buf: &[u8]) -> Result<Option<(Record, usize)>> {
    // Need at least the header
    if buf.len() < HEADER_SIZE {
        return Ok(None);
    }

    // Validate opening magic
    if buf[..4] != MAGIC {
        return Err(Error::InvalidMagic);
    }

    // Read length
    let length = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;

    // Read stored checksum
    let stored_checksum = u32::from_le_bytes(buf[8..12].try_into().unwrap());

    // Check we have enough bytes for payload + trailer
    let total = HEADER_SIZE + length;
    if buf.len() < total {
        return Ok(None);
    }

    // Extract payload
    let payload = buf[HEADER_SIZE..HEADER_SIZE + length].to_vec();

    // Verify checksum
    let actual_checksum = compute_checksum(&payload);
    if actual_checksum != stored_checksum {
        return Err(Error::ChecksumMismatch {
            expected: stored_checksum,
            actual: actual_checksum,
        });
    }

    Ok(Some((Record { payload }, total)))
}

/// Compute CRC32C checksum of the given bytes.
#[inline]
pub(crate) fn compute_checksum(data: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let payload = b"hello world";
        let encoded = encode(payload).unwrap();
        let (record, consumed) = decode(&encoded).unwrap().unwrap();

        assert_eq!(record.as_bytes(), payload);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn decode_empty_buf_returns_none() {
        assert!(decode(&[]).unwrap().is_none());
    }

    #[test]
    fn decode_partial_buf_returns_none() {
        let payload = b"hello";
        let encoded = encode(payload).unwrap();
        // Provide only half the bytes
        assert!(decode(&encoded[..encoded.len() / 2]).unwrap().is_none());
    }

    #[test]
    fn decode_bad_magic_returns_error() {
        let payload = b"hello";
        let mut encoded = encode(payload).unwrap();
        // Corrupt the magic bytes
        encoded[0] = 0xFF;
        assert!(matches!(decode(&encoded), Err(Error::InvalidMagic)));
    }

    #[test]
    fn decode_bad_checksum_returns_error() {
        let payload = b"hello";
        let mut encoded = encode(payload).unwrap();
        // Corrupt the payload (after the header)
        encoded[HEADER_SIZE] ^= 0xFF;
        assert!(matches!(
            decode(&encoded),
            Err(Error::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn empty_payload_roundtrip() {
        let payload = b"";
        let encoded = encode(payload).unwrap();
        let (record, _) = decode(&encoded).unwrap().unwrap();
        assert!(record.is_empty());
    }

    #[test]
    fn large_payload_roundtrip() {
        let payload = vec![0xABu8; 1024 * 1024]; // 1MB
        let encoded = encode(&payload).unwrap();
        let (record, _) = decode(&encoded).unwrap().unwrap();
        assert_eq!(record.as_bytes(), payload.as_slice());
    }
}
