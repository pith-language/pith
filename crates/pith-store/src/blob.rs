use pith_ids::ContentId;
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq)]
pub struct Blob {
    id: ContentId,
    bytes: Arc<[u8]>,
}

impl Blob {
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        let bytes = bytes.into();
        Self {
            id: ContentId::of_blob(&bytes),
            bytes,
        }
    }

    pub fn id(&self) -> ContentId {
        self.id
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl std::fmt::Debug for Blob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Blob")
            .field("id", &self.id)
            .field("len", &self.bytes.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_derived_from_bytes() {
        let blob = Blob::new(&b"hello"[..]);

        assert_eq!(blob.id(), ContentId::of_blob(b"hello"));
        assert_eq!(blob.as_bytes(), b"hello");
    }
}
