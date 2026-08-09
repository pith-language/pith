//! The canonical wire primitives shared by every versioned pith encoding.
//!
//! [`crate::manifest`] owns the write half: a little-endian `u64` length
//! followed by raw bytes, so two encodings that differ only by field boundaries
//! cannot collide. This module owns the matching read half and re-exports both,
//! so a persistence adapter outside `pith-core` builds on the same primitive
//! rather than a second implementation of it.
//!
//! A *digest* manifest and a *storage* encoding are different contracts even
//! when they share these primitives. Decision 0023 keeps the digest manifest
//! format owned by its producer; decision 0024 makes the storage encoding a
//! separately versioned contract. Sharing the primitive does not couple them.

pub use crate::manifest::{encode_bytes, encode_length, encode_str};
pub use crate::value_codec::CanonicalDecodeError;

use std::mem::size_of;

use pith_ids::{ContentDigest, ContentId, DIGEST_LEN};

/// A cursor over a canonical encoding.
///
/// Every read consumes from the front and fails with
/// [`CanonicalDecodeError::Truncated`] rather than panicking, which is what
/// lets adapters decode untrusted stored bytes without an indexing panic.
pub struct CanonicalReader<'encoded> {
    remaining: &'encoded [u8],
}

impl<'encoded> CanonicalReader<'encoded> {
    #[must_use]
    pub const fn new(encoded: &'encoded [u8]) -> Self {
        Self { remaining: encoded }
    }

    /// Read and check a leading encoding-version byte.
    ///
    /// # Errors
    /// [`CanonicalDecodeError::UnsupportedVersion`] when the byte is not
    /// `expected`, or [`CanonicalDecodeError::Truncated`] when absent.
    pub fn read_version(&mut self, expected: u8) -> Result<(), CanonicalDecodeError> {
        let version = self.read_byte()?;
        if version != expected {
            return Err(CanonicalDecodeError::UnsupportedVersion { version });
        }
        Ok(())
    }

    /// # Errors
    /// [`CanonicalDecodeError::Truncated`] when no byte remains.
    pub fn read_byte(&mut self) -> Result<u8, CanonicalDecodeError> {
        let Some((byte, remaining)) = self.remaining.split_first() else {
            return Err(CanonicalDecodeError::Truncated);
        };
        self.remaining = remaining;
        Ok(*byte)
    }

    /// # Errors
    /// [`CanonicalDecodeError::InvalidBoolean`] for a byte other than 0 or 1.
    pub fn read_bool(&mut self) -> Result<bool, CanonicalDecodeError> {
        match self.read_byte()? {
            0 => Ok(false),
            1 => Ok(true),
            byte => Err(CanonicalDecodeError::InvalidBoolean { byte }),
        }
    }

    /// # Errors
    /// [`CanonicalDecodeError::Truncated`] when fewer than eight bytes remain.
    pub fn read_int(&mut self) -> Result<i64, CanonicalDecodeError> {
        let bytes = self.take(size_of::<i64>())?;
        let mut encoded = [0; size_of::<i64>()];
        encoded.copy_from_slice(bytes);
        Ok(i64::from_le_bytes(encoded))
    }

    /// # Errors
    /// [`CanonicalDecodeError::Truncated`] when fewer than eight bytes remain,
    /// or [`CanonicalDecodeError::LengthOutOfRange`] when the encoded length is
    /// not representable on this platform.
    pub fn read_length(&mut self) -> Result<usize, CanonicalDecodeError> {
        let bytes = self.take(size_of::<u64>())?;
        let mut encoded = [0; size_of::<u64>()];
        encoded.copy_from_slice(bytes);
        let length = u64::from_le_bytes(encoded);
        usize::try_from(length).map_err(|_| CanonicalDecodeError::LengthOutOfRange { length })
    }

    /// # Errors
    /// [`CanonicalDecodeError::Truncated`] or
    /// [`CanonicalDecodeError::LengthOutOfRange`].
    pub fn read_bytes(&mut self) -> Result<&'encoded [u8], CanonicalDecodeError> {
        let length = self.read_length()?;
        self.take(length)
    }

    /// # Errors
    /// [`CanonicalDecodeError::InvalidUtf8`] when the bytes are not UTF-8, plus
    /// the errors of [`Self::read_bytes`].
    pub fn read_text(&mut self) -> Result<&'encoded str, CanonicalDecodeError> {
        std::str::from_utf8(self.read_bytes()?).map_err(|_| CanonicalDecodeError::InvalidUtf8)
    }

    /// # Errors
    /// [`CanonicalDecodeError::Truncated`] when fewer than [`DIGEST_LEN`] bytes
    /// remain.
    pub fn read_digest(&mut self) -> Result<ContentDigest, CanonicalDecodeError> {
        let bytes = self.take(DIGEST_LEN)?;
        let mut digest = [0; DIGEST_LEN];
        digest.copy_from_slice(bytes);
        Ok(ContentDigest::from_bytes(digest))
    }

    /// # Errors
    /// The errors of [`Self::read_digest`].
    pub fn read_content_id(&mut self) -> Result<ContentId, CanonicalDecodeError> {
        Ok(ContentId::from_digest(self.read_digest()?))
    }

    /// Read a length-prefixed sequence, decoding each element with `element`.
    ///
    /// # Errors
    /// Propagates the element decoder's error, plus the errors of
    /// [`Self::read_length`].
    pub fn read_sequence<T>(
        &mut self,
        mut element: impl FnMut(&mut Self) -> Result<T, CanonicalDecodeError>,
    ) -> Result<Box<[T]>, CanonicalDecodeError> {
        let length = self.read_length()?;
        let mut items = Vec::with_capacity(length.min(self.remaining.len()));
        for _ in 0..length {
            items.push(element(self)?);
        }
        Ok(items.into_boxed_slice())
    }

    /// # Errors
    /// [`CanonicalDecodeError::Truncated`] when fewer than `length` bytes
    /// remain.
    pub fn take(&mut self, length: usize) -> Result<&'encoded [u8], CanonicalDecodeError> {
        if self.remaining.len() < length {
            return Err(CanonicalDecodeError::Truncated);
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    /// Assert the encoding is fully consumed.
    ///
    /// # Errors
    /// [`CanonicalDecodeError::TrailingBytes`] when bytes remain.
    pub fn finish(self) -> Result<(), CanonicalDecodeError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(CanonicalDecodeError::TrailingBytes)
        }
    }
}

/// Append a length-prefixed sequence: the element count, then each element.
pub fn encode_sequence<T>(
    encoded: &mut Vec<u8>,
    items: &[T],
    mut element: impl FnMut(&mut Vec<u8>, &T),
) {
    encode_length(encoded, items.len());
    for item in items {
        element(encoded, item);
    }
}

// ---------------------------------------------------------------------------
// Shared record primitives
//
// Encodings outside `pith-core` (the engine-state storage codec) need the same
// byte layout for these small shared types as the action-contract encoding
// uses. Defining them once here keeps a capability or a content reference from
// acquiring two spellings on disk.
// ---------------------------------------------------------------------------

use crate::action::{ActionInputContent, CapabilityRequirement, Content, OutputKind};

const TAG_BLOB: u8 = 0;
const TAG_TREE: u8 = 1;

#[must_use]
pub const fn output_kind_tag(kind: OutputKind) -> u8 {
    match kind {
        OutputKind::Blob => TAG_BLOB,
        OutputKind::Tree => TAG_TREE,
    }
}

/// # Errors
/// [`CanonicalDecodeError::UnknownValueTag`] for a tag that is neither blob nor
/// tree.
pub fn read_output_kind(
    reader: &mut CanonicalReader<'_>,
) -> Result<OutputKind, CanonicalDecodeError> {
    match reader.read_byte()? {
        TAG_BLOB => Ok(OutputKind::Blob),
        TAG_TREE => Ok(OutputKind::Tree),
        tag => Err(CanonicalDecodeError::UnknownValueTag { tag }),
    }
}

pub fn encode_content(encoded: &mut Vec<u8>, content: &ActionInputContent) {
    encoded.push(output_kind_tag(content.kind()));
    let (Content::Blob(id) | Content::Tree(id)) = content;
    encoded.extend_from_slice(id.digest().as_bytes());
}

/// # Errors
/// The errors of [`read_output_kind`] and [`CanonicalReader::read_content_id`].
pub fn read_content(
    reader: &mut CanonicalReader<'_>,
) -> Result<ActionInputContent, CanonicalDecodeError> {
    let kind = read_output_kind(reader)?;
    let id = reader.read_content_id()?;
    Ok(match kind {
        OutputKind::Blob => Content::Blob(id),
        OutputKind::Tree => Content::Tree(id),
    })
}

pub fn encode_capabilities(encoded: &mut Vec<u8>, capabilities: &[CapabilityRequirement]) {
    encode_sequence(encoded, capabilities, |out, capability| {
        encode_str(out, &capability.name);
        encode_str(out, &capability.scope);
    });
}

/// # Errors
/// The errors of [`CanonicalReader::read_sequence`] and
/// [`CanonicalReader::read_text`].
pub fn read_capabilities(
    reader: &mut CanonicalReader<'_>,
) -> Result<Box<[CapabilityRequirement]>, CanonicalDecodeError> {
    reader.read_sequence(|reader| {
        let name = Box::<str>::from(reader.read_text()?);
        let scope = Box::<str>::from(reader.read_text()?);
        Ok(CapabilityRequirement { name, scope })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sequence_round_trips_through_its_length_prefix() {
        let mut encoded = Vec::new();
        encode_sequence(&mut encoded, &["a", "bc", ""], |out, item| {
            encode_str(out, item);
        });

        let mut reader = CanonicalReader::new(&encoded);
        let decoded = reader
            .read_sequence(|reader| reader.read_text().map(Box::<str>::from))
            .expect("the sequence decodes");
        reader.finish().expect("the encoding is fully consumed");
        assert_eq!(decoded.as_ref(), ["a".into(), "bc".into(), "".into()]);
    }

    #[test]
    fn a_declared_length_longer_than_the_input_is_truncated_not_a_panic() {
        // A stored encoding is untrusted input: an absurd element count must
        // become an error rather than an allocation or an index panic.
        let mut encoded = Vec::new();
        encode_length(&mut encoded, usize::MAX);

        let mut reader = CanonicalReader::new(&encoded);
        let decoded = reader.read_sequence(|reader| reader.read_byte());
        assert_eq!(decoded.err(), Some(CanonicalDecodeError::Truncated));
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let encoded = [0u8, 1];
        let mut reader = CanonicalReader::new(&encoded);
        assert_eq!(reader.read_byte(), Ok(0));
        assert_eq!(
            reader.finish().err(),
            Some(CanonicalDecodeError::TrailingBytes)
        );
    }
}
