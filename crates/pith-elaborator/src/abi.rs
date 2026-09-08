use pith_core::{
    DeclarationTable, Interface,
    manifest::{encode_bytes, encode_length, encode_str},
};
use pith_hir::RuleCategory;
use pith_ids::ModuleAbiDigest;

pub const GRAMMAR_VERSION: u8 = 1;

pub struct RuleSignature {
    pub category: RuleCategory,
    pub interface: Interface,
}

/// The imported subject/ABI pairs a module elaborated against, in the one
/// canonical form both the ABI digest and the interface surface encode:
/// sorted by subject, one entry per subject (whichever aliases reached
/// it). A subject arriving with two different digests is refused;
/// canonicalization does not resolve conflicts, callers do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportedAbis {
    entries: Box<[(Box<str>, ModuleAbiDigest)]>,
}

/// One subject, two different ABI digests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictingImports {
    pub subject: Box<str>,
    pub first: ModuleAbiDigest,
    pub second: ModuleAbiDigest,
}

impl std::fmt::Display for ConflictingImports {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "the subject {} is imported with two different ABI digests",
            self.subject
        )
    }
}

impl std::error::Error for ConflictingImports {}

impl ImportedAbis {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: Box::new([]),
        }
    }

    /// # Errors
    /// Returns [`ConflictingImports`] when one subject arrives with two
    /// different digests.
    pub fn new(
        imports: impl IntoIterator<Item = (Box<str>, ModuleAbiDigest)>,
    ) -> Result<Self, ConflictingImports> {
        let mut entries = imports.into_iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        for [earlier, later] in entries.array_windows() {
            if earlier.0 == later.0 && earlier.1 != later.1 {
                return Err(ConflictingImports {
                    subject: earlier.0.clone(),
                    first: earlier.1,
                    second: later.1,
                });
            }
        }
        entries.dedup_by(|left, right| left.0 == right.0);
        Ok(Self {
            entries: entries.into(),
        })
    }

    #[must_use]
    pub fn as_slice(&self) -> &[(Box<str>, ModuleAbiDigest)] {
        &self.entries
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, ModuleAbiDigest)> {
        self.entries
            .iter()
            .map(|(subject, digest)| (subject.as_ref(), *digest))
    }
}

/// The ABI of a module: its identity, its declarations, the imported
/// subjects it elaborated against, and the rules it provides. A local
/// binding name is an elaboration input and a tooling sidecar, not a
/// semantic fact, so renaming an alias leaves this digest unchanged.
pub fn abi_digest(
    module: &str,
    table: &DeclarationTable,
    imports: &ImportedAbis,
    signatures: &[RuleSignature],
) -> ModuleAbiDigest {
    let imports = imports.as_slice();
    let mut manifest = Vec::new();
    encode_str(&mut manifest, module);
    manifest.push(GRAMMAR_VERSION);
    manifest.push(pith_core::ENCODING_VERSION);

    encode_length(&mut manifest, table.len());
    for declaration in table.iter() {
        manifest.extend_from_slice(declaration.digest().digest().as_bytes());
    }

    encode_length(&mut manifest, imports.len());
    for (subject, digest) in imports {
        encode_str(&mut manifest, subject);
        manifest.extend_from_slice(digest.digest().as_bytes());
    }

    let mut provided: Vec<(u8, Vec<u8>)> = signatures
        .iter()
        .map(|signature| {
            (
                signature.category.abi_tag(),
                signature.interface.encode_canonical(),
            )
        })
        .collect();
    provided.sort();
    encode_length(&mut manifest, provided.len());
    for (category, interface) in provided {
        manifest.push(category);
        encode_bytes(&mut manifest, &interface);
    }

    ModuleAbiDigest::of_manifest(&manifest)
}
