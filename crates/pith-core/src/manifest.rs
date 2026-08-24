//! Canonical length-prefixed encoding for stable manifests.
//!
//! Every digest in the kernel hashes a canonical byte manifest. Variable-length
//! fields are encoded as a little-endian `u64` length followed by the raw bytes,
//! so two manifests that differ only by field boundaries cannot collide. This
//! is the single implementation of that primitive for `pith-core`; both

/// Append a little-endian `u64` length to `manifest`.
pub fn encode_length(manifest: &mut Vec<u8>, length: usize) {
    manifest.extend_from_slice(&(length as u64).to_le_bytes());
}

/// Append a length-prefixed byte blob: `u64` length, then the raw bytes.
pub fn encode_bytes(manifest: &mut Vec<u8>, bytes: &[u8]) {
    encode_length(manifest, bytes.len());
    manifest.extend_from_slice(bytes);
}

/// Append a length-prefixed UTF-8 string. Convenience over [`encode_bytes`].
pub fn encode_str(manifest: &mut Vec<u8>, text: &str) {
    encode_bytes(manifest, text.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_is_eight_little_endian_bytes() {
        let mut manifest = Vec::new();
        encode_length(&mut manifest, 0x0102030405060708);
        assert_eq!(manifest, [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]);
    }

    #[test]
    fn bytes_are_length_prefixed() {
        let mut manifest = Vec::new();
        encode_bytes(&mut manifest, b"abc");
        assert_eq!(manifest, [3, 0, 0, 0, 0, 0, 0, 0, b'a', b'b', b'c']);
    }

    #[test]
    fn distinct_lengths_keep_distinct_boundaries() {
        let mut left = Vec::new();
        encode_bytes(&mut left, b"ab");
        encode_bytes(&mut left, b"c");

        let mut right = Vec::new();
        encode_bytes(&mut right, b"a");
        encode_bytes(&mut right, b"bc");

        assert_ne!(left, right);
    }

    #[test]
    fn empty_bytes_still_record_a_length() {
        let mut manifest = Vec::new();
        encode_bytes(&mut manifest, b"");
        assert_eq!(manifest, [0u8; 8]);
    }
}
