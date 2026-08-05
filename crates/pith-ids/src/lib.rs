//! The five identity types of the pith kernel (decisions 0005, 0013).
//!
//! Each is a distinct brand-typed id, so a `ContentId` does not type-check
//! where a `SemanticId` is wanted.

use pith_arena::define_arena;

define_arena!(
    SemanticId,
    SemanticArena,
    SemanticBrand,
    "Semantic identity: what a value represents across implementations or refactors."
);

define_arena!(
    ComputationId,
    ComputationArena,
    ComputationBrand,
    "Computation identity: a rule application plus the relevant inputs. Two computations with \
     the same identity must produce the same result; the cache keys on this (decision 0019)."
);

define_arena!(
    ContentId,
    ContentArena,
    ContentBrand,
    "Content identity: immutable bytes or a canonical structured value, by digest."
);

define_arena!(
    ExternalId,
    ExternalArena,
    ExternalBrand,
    "External identity: an identifier assigned by an external system. Can change while the \
     underlying object persists (decision 0013)."
);

define_arena!(
    ManagedObjectId,
    ManagedObjectArena,
    ManagedObjectBrand,
    "Managed-object identity: a durable external object a deployment owns and mutates across \
     observations and platform re-creation (decision 0013)."
);

#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentDigest([u8; 32]);

impl ContentDigest {
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for ContentDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContentDigest(")?;
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_bytes_produce_equal_digests() {
        assert_eq!(
            ContentDigest::of_bytes(b"hello"),
            ContentDigest::of_bytes(b"hello")
        );
        assert_ne!(
            ContentDigest::of_bytes(b"hello"),
            ContentDigest::of_bytes(b"world")
        );
    }

    #[test]
    fn identity_types_are_distinct() {
        fn name<T>() -> &'static str {
            std::any::type_name::<T>()
        }
        let ids = [
            name::<SemanticId>(),
            name::<ComputationId>(),
            name::<ContentId>(),
            name::<ExternalId>(),
            name::<ManagedObjectId>(),
        ];
        for (i, a) in ids.iter().enumerate() {
            for b in ids.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn digest_debug_is_hex() {
        let d = ContentDigest::of_bytes(b"");
        let s = format!("{d:?}");
        assert!(s.starts_with("ContentDigest("));
        assert!(s.ends_with(')'));
        let hex = &s["ContentDigest(".len()..s.len() - 1];
        assert_eq!(hex.len(), 64);
    }
}
