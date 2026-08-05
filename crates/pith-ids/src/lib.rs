//! The five identity types of the pith kernel (decisions 0005, 0013).
//!
//! Each identity is a distinct type, so a `ContentId` does not type-check where
//! a `SemanticId` is wanted.

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
    "Arena-local computation identity for an interned rule application and its relevant inputs. \
     Persistent construction across engine instances remains unresolved."
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

#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentId(ContentDigest);

impl ContentId {
    pub const fn from_digest(digest: ContentDigest) -> Self {
        Self(digest)
    }

    pub fn of_blob(bytes: &[u8]) -> Self {
        Self::with_domain(b"pith:blob:v1\0", bytes)
    }

    pub fn of_tree(manifest: &[u8]) -> Self {
        Self::with_domain(b"pith:tree:v1\0", manifest)
    }

    fn with_domain(domain: &[u8], bytes: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(domain);
        hasher.update(bytes);
        Self(ContentDigest(hasher.finalize().into()))
    }

    pub fn digest(self) -> ContentDigest {
        self.0
    }
}

impl std::fmt::Debug for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContentId({:?})", self.0)
    }
}

/// Stable digest of a canonical declared action contract.
///
/// This is a persistent specialization of computation identity, not content
/// identity: equal action contracts have equal digests even before they run.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionSpecDigest(ContentDigest);

impl ActionSpecDigest {
    pub fn of_manifest(manifest: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"pith:action:v1\0");
        hasher.update(manifest);
        Self(ContentDigest(hasher.finalize().into()))
    }

    pub fn digest(self) -> ContentDigest {
        self.0
    }
}

impl std::fmt::Debug for ActionSpecDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ActionSpecDigest({:?})", self.0)
    }
}

/// Stable digest of the current provisional identity for a pure rule.
///
/// This remains provisional until rules carry semantic identity independent
/// of their current metadata.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProvisionalRuleIdentity(ContentDigest);

impl ProvisionalRuleIdentity {
    pub fn of_manifest(manifest: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"pith:pure-rule:provisional:v1\0");
        hasher.update(manifest);
        Self(ContentDigest(hasher.finalize().into()))
    }

    pub fn digest(self) -> ContentDigest {
        self.0
    }
}

impl std::fmt::Debug for ProvisionalRuleIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProvisionalRuleIdentity({:?})", self.0)
    }
}

/// Stable digest of a canonical pure rule application.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PureComputationDigest(ContentDigest);

impl PureComputationDigest {
    pub fn of_manifest(manifest: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"pith:pure-computation:v1\0");
        hasher.update(manifest);
        Self(ContentDigest(hasher.finalize().into()))
    }

    pub fn digest(self) -> ContentDigest {
        self.0
    }
}

impl std::fmt::Debug for PureComputationDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PureComputationDigest({:?})", self.0)
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

    #[test]
    fn blob_identity_is_domain_separated() {
        assert_eq!(ContentId::of_blob(b"same"), ContentId::of_blob(b"same"));
        assert_ne!(ContentId::of_blob(b"same"), ContentId::of_blob(b"other"));
        assert_ne!(
            ContentId::of_blob(b"same").digest(),
            ContentDigest::of_bytes(b"same")
        );
        assert_ne!(ContentId::of_blob(b"same"), ContentId::of_tree(b"same"));
    }

    #[test]
    fn content_identity_can_be_reconstructed_from_a_known_digest() {
        let id = ContentId::of_blob(b"remote");

        assert_eq!(ContentId::from_digest(id.digest()), id);
    }

    #[test]
    fn action_spec_digest_is_domain_separated() {
        let action = ActionSpecDigest::of_manifest(b"same");

        assert_eq!(action, ActionSpecDigest::of_manifest(b"same"));
        assert_ne!(action.digest(), ContentDigest::of_bytes(b"same"));
        assert_ne!(action.digest(), ContentId::of_blob(b"same").digest());
    }

    #[test]
    fn pure_identity_digests_are_domain_separated() {
        let rule = ProvisionalRuleIdentity::of_manifest(b"same");
        let computation = PureComputationDigest::of_manifest(b"same");

        assert_ne!(rule.digest(), computation.digest());
        assert_ne!(computation.digest(), ContentDigest::of_bytes(b"same"));
        assert_ne!(computation.digest(), ContentId::of_blob(b"same").digest());
    }
}
