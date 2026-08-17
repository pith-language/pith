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

/// Length in bytes of every blake3-derived digest in the kernel. The single
/// source of truth for the `[u8; DIGEST_LEN]` width used by `ContentDigest`,
/// the store's manifest reader, and tests.
pub const DIGEST_LEN: usize = 32;

/// The name of the hash function behind every digest in the kernel, as the
/// written forms' digest prefix spells it. One source for both: the prefix
/// is bound to this name by test, so the written spelling and the hasher
/// cannot drift apart silently.
pub const DIGEST_ALGORITHM: &str = "blake3";

#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentDigest([u8; DIGEST_LEN]);

impl ContentDigest {
    pub const fn from_bytes(bytes: [u8; DIGEST_LEN]) -> Self {
        Self(bytes)
    }

    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).into())
    }

    pub fn as_bytes(&self) -> &[u8; DIGEST_LEN] {
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
        Self::from_digest(Self::with_domain(domain::CONTENT_BLOB, bytes))
    }

    pub fn of_tree(manifest: &[u8]) -> Self {
        Self::from_digest(Self::with_domain(domain::CONTENT_TREE, manifest))
    }

    pub fn digest(self) -> ContentDigest {
        self.0
    }

    /// Hash `bytes` under a NUL-terminated domain-separation prefix. The prefix
    /// is self-delimiting, so no length separator is needed between it and the
    /// payload. Shared by every digest kind in this crate.
    pub(crate) fn with_domain(prefix: &[u8], bytes: &[u8]) -> ContentDigest {
        let mut hasher = blake3::Hasher::new();
        hasher.update(prefix);
        hasher.update(bytes);
        ContentDigest(hasher.finalize().into())
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
        Self(ContentId::with_domain(domain::ACTION_SPEC, manifest))
    }

    /// Restore a digest read back from a persistence adapter.
    ///
    /// Derivation is the only way to *establish* this identity; restoration
    /// asserts one that was already derived and then stored. Adapters
    /// implementing durable engine state (decision 0024) need it because a
    /// stored record holds the digest, not the manifest that produced it.
    pub const fn from_digest(digest: ContentDigest) -> Self {
        Self(digest)
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
        hasher.update(domain::RULE_IDENTITY);
        hash_bytes(&mut hasher, module_identity.as_bytes());
        hash_bytes(&mut hasher, declaration_identity.as_bytes());
        Self(ContentDigest(hasher.finalize().into()))
    }

    /// Restore an identity read back from a persistence adapter. See
    /// [`ActionSpecDigest::from_digest`] for why restoration is distinct from
    /// derivation.
    pub const fn from_digest(digest: ContentDigest) -> Self {
        Self(digest)
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
        hasher.update(domain::RULE_REVISION);
        hasher.update(rule_identity.digest().as_bytes());
        hash_bytes(&mut hasher, revision_manifest);
        Self {
            rule_identity,
            digest: ContentDigest(hasher.finalize().into()),
        }
    }

    /// Restore a revision read back from a persistence adapter. Both halves are
    /// stored, because the revision digest commits to the rule identity but
    /// cannot be inverted to recover it. See [`ActionSpecDigest::from_digest`]
    /// for why restoration is distinct from derivation.
    pub const fn from_parts(rule_identity: RuleIdentity, digest: ContentDigest) -> Self {
        Self {
            rule_identity,
            digest,
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
        Self(ContentId::with_domain(domain::PURE_COMPUTATION, manifest))
    }

    /// Restore a digest read back from a persistence adapter. See
    /// [`ActionSpecDigest::from_digest`] for why restoration is distinct from
    /// derivation.
    pub const fn from_digest(digest: ContentDigest) -> Self {
        Self(digest)
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

/// Stable digest of one declaration: its coordinate, its kind, and the canonical
/// encoding of its body (decision 0047).
///
/// What it covers is chosen so that it moves for a change a reader can observe
/// and not otherwise. A doc-comment edit, the declaration's position in its
/// module's table, and formatting are all outside it; the representation type of
/// a nominal, the constructor set of a sum, and a constructor payload's type are
/// all inside it. That asymmetry is the Dhall 1.17.0 lesson: a digest whose basis
/// includes what no reader can see breaks compatibility for nothing.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeclarationDigest(ContentDigest);

impl DeclarationDigest {
    pub fn of_manifest(manifest: &[u8]) -> Self {
        Self(ContentId::with_domain(domain::DECLARATION, manifest))
    }

    /// Restore a digest read back from a persistence adapter. See
    /// [`ActionSpecDigest::from_digest`] for why restoration is distinct from
    /// derivation.
    pub const fn from_digest(digest: ContentDigest) -> Self {
        Self(digest)
    }

    pub fn digest(self) -> ContentDigest {
        self.0
    }
}

impl std::fmt::Debug for DeclarationDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DeclarationDigest({:?})", self.0)
    }
}

/// Stable digest of a canonical action rule application: the request that was
/// asked for and the contract the rule planned from it (decision 0031).
///
/// The request half of action identity. What the executor resolved — the
/// platform it ran on, the confinement it installed, the content it produced —
/// is knowable only after execution, so decision 0031 tests those facts when a
/// recorded attempt is considered for reuse and keeps them out of this digest.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionComputationDigest(ContentDigest);

impl ActionComputationDigest {
    pub fn of_manifest(manifest: &[u8]) -> Self {
        Self(ContentId::with_domain(domain::ACTION_COMPUTATION, manifest))
    }

    /// Restore a digest read back from a persistence adapter. See
    /// [`ActionSpecDigest::from_digest`] for why restoration is distinct from
    /// derivation.
    pub const fn from_digest(digest: ContentDigest) -> Self {
        Self(digest)
    }

    pub fn digest(self) -> ContentDigest {
        self.0
    }
}

impl std::fmt::Debug for ActionComputationDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ActionComputationDigest({:?})", self.0)
    }
}

/// Domain-separation prefixes for blake3 hashing. Each prefix is a NUL-terminated
/// byte literal of the form `pith:<kind>:<version>\0`, where every prefix shares
/// the same `<version>` segment (`v1` today).
///
/// These are the single source of truth for which bytes identify each digest
/// kind: adding a digest kind means adding one prefix here and one call site,
/// and a prefix must never be rederived inline at a hashing site. The
/// `prefixes_follow_the_version_template` test refuses to compile/run if the
/// version segments drift apart.
mod domain {
    pub const CONTENT_BLOB: &[u8] = b"pith:blob:v1\0";
    pub const CONTENT_TREE: &[u8] = b"pith:tree:v1\0";
    pub const ACTION_SPEC: &[u8] = b"pith:action:v1\0";
    pub const RULE_IDENTITY: &[u8] = b"pith:rule-identity:v1\0";
    pub const RULE_REVISION: &[u8] = b"pith:rule-revision:v1\0";
    pub const PURE_COMPUTATION: &[u8] = b"pith:pure-computation:v1\0";
    pub const ACTION_COMPUTATION: &[u8] = b"pith:action-computation:v1\0";
    pub const DECLARATION: &[u8] = b"pith:declaration:v1\0";

    /// Every prefix in this module. Distinctness and a shared version
    /// segment are checked over this one slice, so a new digest kind is
    /// covered by adding it here.
    #[cfg(test)]
    pub const ALL: &[&[u8]] = &[
        CONTENT_BLOB,
        CONTENT_TREE,
        ACTION_SPEC,
        RULE_IDENTITY,
        RULE_REVISION,
        PURE_COMPUTATION,
        ACTION_COMPUTATION,
        DECLARATION,
    ];
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
    fn action_computation_digest_is_separated_from_the_other_action_digest() {
        // `ActionSpecDigest` identifies the contract; `ActionComputationDigest`
        // identifies the rule application that planned it. The same manifest
        // under both must not collide.
        let computation = ActionComputationDigest::of_manifest(b"same");

        assert_eq!(computation, ActionComputationDigest::of_manifest(b"same"));
        assert_ne!(
            computation.digest(),
            ActionSpecDigest::of_manifest(b"same").digest()
        );
        assert_ne!(
            computation.digest(),
            PureComputationDigest::of_manifest(b"same").digest()
        );
        assert_ne!(computation.digest(), ContentDigest::of_bytes(b"same"));
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

    #[test]
    fn digest_length_is_32_bytes() {
        // The width is a public contract: the store's manifest reader and any
        // persistent encoding depend on it.
        assert_eq!(DIGEST_LEN, 32);
        assert_eq!(std::mem::size_of::<ContentDigest>(), DIGEST_LEN);
        assert_eq!(
            ContentDigest::from_bytes([0u8; DIGEST_LEN])
                .as_bytes()
                .len(),
            DIGEST_LEN
        );
    }

    #[test]
    fn domain_prefixes_are_distinct() {
        // A collision here would mean two digest kinds hash into the same
        // namespace, silently breaking domain separation.
        let prefixes = domain::ALL;
        for (i, a) in prefixes.iter().enumerate() {
            for b in prefixes.iter().skip(i + 1) {
                assert_ne!(a, b, "two domain prefixes collide");
            }
        }
    }

    #[test]
    fn prefixes_follow_the_version_template() {
        let prefixes = domain::ALL;

        let version_of = |prefix: &[u8]| -> String {
            let s = std::str::from_utf8(prefix).expect("prefixes are ASCII + NUL");
            assert!(s.starts_with("pith:"), "{s:?}: missing pith: header");
            assert!(s.ends_with('\0'), "{s:?}: missing NUL terminator");
            let body = &s["pith:".len()..s.len() - 1];
            let (_, version) = body
                .rsplit_once(':')
                .expect("prefix body has a `<kind>:<version>` shape");
            version.to_string()
        };

        let mut versions = prefixes.iter().map(|prefix| version_of(prefix));
        let first = versions.next().expect("there is at least one prefix");
        assert_eq!(
            first, "v1",
            "version segment is no longer v1; update this test deliberately"
        );
        for version in versions {
            assert_eq!(
                version, first,
                "a prefix carries a different version segment"
            );
        }
    }
}
