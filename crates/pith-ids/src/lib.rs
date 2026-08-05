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
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for ContentDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
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

/// Stable identity of a semantic rule declaration (decision 0023).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleIdentity(ContentDigest);

impl RuleIdentity {
    pub fn of_module_declaration(module_identity: &str, declaration_identity: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"pith:rule-identity:v1\0");
        hash_bytes(&mut hasher, module_identity.as_bytes());
        hash_bytes(&mut hasher, declaration_identity.as_bytes());
        Self(ContentDigest(hasher.finalize().into()))
    }

    pub fn digest(self) -> ContentDigest {
        self.0
    }
}

impl std::fmt::Debug for RuleIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RuleIdentity({:?})", self.0)
    }
}

/// Cache-invalidating revision of a rule's executable semantics (decision 0023).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleRevision {
    rule_identity: RuleIdentity,
    digest: ContentDigest,
}

impl RuleRevision {
    pub fn of_manifest(rule_identity: RuleIdentity, revision_manifest: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"pith:rule-revision:v1\0");
        hasher.update(rule_identity.digest().as_bytes());
        hash_bytes(&mut hasher, revision_manifest);
        Self {
            rule_identity,
            digest: ContentDigest(hasher.finalize().into()),
        }
    }

    pub fn rule_identity(self) -> RuleIdentity {
        self.rule_identity
    }

    pub fn digest(self) -> ContentDigest {
        self.digest
    }
}

impl std::fmt::Debug for RuleRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RuleRevision({:?})", self.digest)
    }
}

fn hash_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
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
        assert_eq!(d.to_string(), hex);
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
    fn rule_and_pure_computation_digests_are_domain_separated() {
        let identity = RuleIdentity::of_module_declaration("same", "same");
        let revision = RuleRevision::of_manifest(identity, b"same");
        let computation = PureComputationDigest::of_manifest(b"same");

        assert_ne!(identity.digest(), revision.digest());
        assert_ne!(identity.digest(), computation.digest());
        assert_ne!(revision.digest(), computation.digest());
        assert_ne!(computation.digest(), ContentDigest::of_bytes(b"same"));
        assert_ne!(computation.digest(), ContentId::of_blob(b"same").digest());
    }

    #[test]
    fn rule_identity_encodes_module_and_declaration_boundaries() {
        assert_eq!(
            RuleIdentity::of_module_declaration("module", "rule"),
            RuleIdentity::of_module_declaration("module", "rule")
        );
        assert_ne!(
            RuleIdentity::of_module_declaration("ab", "c"),
            RuleIdentity::of_module_declaration("a", "bc")
        );
    }

    #[test]
    fn rule_revision_is_bound_to_identity_and_manifest() {
        let first_identity = RuleIdentity::of_module_declaration("module", "first");
        let second_identity = RuleIdentity::of_module_declaration("module", "second");

        assert_eq!(
            RuleRevision::of_manifest(first_identity, b"provider-v1"),
            RuleRevision::of_manifest(first_identity, b"provider-v1")
        );
        assert_ne!(
            RuleRevision::of_manifest(first_identity, b"provider-v1"),
            RuleRevision::of_manifest(first_identity, b"provider-v2")
        );
        assert_ne!(
            RuleRevision::of_manifest(first_identity, b"provider-v1"),
            RuleRevision::of_manifest(second_identity, b"provider-v1")
        );
        assert_eq!(
            RuleRevision::of_manifest(first_identity, b"provider-v1").rule_identity(),
            first_identity
        );
    }
}
