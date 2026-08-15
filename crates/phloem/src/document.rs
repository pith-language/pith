//! The lock document: what a written lock is a projection of (decision 0041).
//!
//! The document is a value that holds the entries and, beside them, every
//! input a selection depended on that the declarations do not already hold:
//! the resolver revision, the declared version scheme, the preference list,
//! and the digest of the candidate universe the resolution ran against. The
//! budget is absent because it shapes whether the deterministic walk
//! finishes, never what it selects. The constraint set is absent because
//! the lock records what resolution adds to the declarations, and
//! staleness against them is a comparison of values rather than a hash.
//!
//! Reading a lock back is both a record and an input: the entries become
//! pins, exact constraints over the coordinates each entry carries, and a
//! header field that moved is the staleness a diff names.

use pith_core::{Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::codec::{blob_field, field_of, record_type, record_value, text_field, value_content_id};
use crate::constraint::{Constraint, Range};
use crate::diag;
use crate::identity::{PackageVersion, version_scheme_type, version_scheme_value};
use crate::lock::LockEntry;
use crate::preference::{PreferenceList, preference_list_from_value, preference_list_value};
use crate::resolution::Resolution;
use crate::source::SourceBinding;
use crate::universe::Candidate;

pub use crate::lockdiff::{LockChange, LockDiff, diff};

/// The digest domain for one revision of a lock document. NUL-terminated so
/// it is self-delimiting against the canonical bytes that follow, mirroring
/// the domain separation `pith-ids` applies to every digest kind it owns.
const LOCK_DOMAIN: &[u8] = b"phloem.lock-v1\0";

const RESOLVER: &str = "resolver";
const SCHEME: &str = "version-scheme";
const UNIVERSE: &str = "universe";
const PREFERENCES: &str = "preferences";
const ENTRIES: &str = "entries";

/// The declared lock-document record type: the resolver revision, the
/// declared version scheme, the candidate-universe digest, the preference
/// list, and the entries, as one value.
#[must_use]
pub fn lock_type() -> Type {
    record_type([
        (RESOLVER, Type::Text),
        (SCHEME, version_scheme_type()),
        (UNIVERSE, Type::Blob),
        (
            PREFERENCES,
            Type::List(Box::new(crate::preference::preference_type())),
        ),
        (ENTRIES, Type::List(Box::new(crate::lock::entry_type()))),
    ])
}

/// One lock document: the entries a resolution selected, with every input
/// the selection depended on that the declarations do not already hold.
///
/// The entries are a set over package identities, held in one canonical
/// order — by each entry's canonical encoding — so the same selection set
/// has one spelling and one digest regardless of the order the search
/// reported. The rendered file orders the same entries by the line's own
/// bytes instead, which is the diff's business rather than the digest's
/// (0041).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lock {
    /// The resolve rule's revision digest, as lowercase hex.
    pub resolver: Box<str>,
    /// The declared version scheme the resolution ran under.
    pub scheme: Box<str>,
    /// The digest of the candidate universe the resolution ran against.
    pub universe: ContentId,
    /// The preference list the selection consumed.
    pub preferences: PreferenceList,
    /// The selected bindings, canonically sorted.
    pub entries: Box<[LockEntry]>,
}

impl Lock {
    /// A lock document over `entries`, sorted into the canonical order the
    /// digest and the rendered file read.
    ///
    /// The entries are a set over package identities, the property `parse`
    /// enforces when it reads a file: a document holding two bindings for one
    /// package would render a file `parse` rejects, which falsifies the
    /// round-trip guarantee on the document's own output. The constructor
    /// refuses the document rather than sorting the conflict away.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the package and both versions
    /// when `entries` binds one package twice.
    pub fn new(
        resolver: impl Into<Box<str>>,
        scheme: impl Into<Box<str>>,
        universe: ContentId,
        preferences: PreferenceList,
        entries: impl Into<Vec<LockEntry>>,
    ) -> PithResult<Self> {
        let mut entries = entries.into();
        entries.sort_by(|left, right| {
            left.to_value()
                .encode_canonical()
                .cmp(&right.to_value().encode_canonical())
        });
        for pair in entries.windows(2) {
            if let [earlier, later] = pair
                && earlier.package.identity() == later.package.identity()
            {
                return Err(diag(format!(
                    "the lock binds `{}` in `{}` twice: version {} and version {}; two \
                     selections of one package is the one conflict the entries' set shape \
                     cannot hold",
                    later.package.identity().name(),
                    later.package.identity().domain().as_str(),
                    earlier.package.version(),
                    later.package.version(),
                )));
            }
        }
        Ok(Self {
            resolver: resolver.into(),
            scheme: scheme.into(),
            universe,
            preferences,
            entries: entries.into(),
        })
    }

    /// The document as a value of the declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (RESOLVER, Value::Text(self.resolver.clone())),
            (SCHEME, version_scheme_value(&self.scheme)),
            (UNIVERSE, Value::Blob(self.universe)),
            (PREFERENCES, preference_list_value(&self.preferences)),
            (
                ENTRIES,
                Value::List(self.entries.iter().map(LockEntry::to_value).collect()),
            ),
        ])
    }

    /// Read a lock document from a value.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the declared type and the
    /// value found when the value is not a lock document.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&lock_type()) {
            return Err(diag(format!(
                "expected a value of the lock document type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(diag(format!(
                "expected a value of the lock document type, found {}",
                value.describe()
            )));
        };
        let resolver = text_field(fields, RESOLVER)?;
        let scheme: Box<str> = match field_of(fields, SCHEME) {
            Some(payload) => crate::identity::version_scheme_name(payload)?.into(),
            None => return Err(diag(format!("the record carried no {SCHEME}"))),
        };
        let universe = blob_field(fields, UNIVERSE)?;
        let preferences = match field_of(fields, PREFERENCES) {
            Some(payload) => preference_list_from_value(payload)?,
            None => PreferenceList(Box::new([])),
        };
        let mut entries = Vec::new();
        if let Some(Value::List(elements)) = field_of(fields, ENTRIES) {
            for element in elements.iter() {
                entries.push(LockEntry::from_value(element)?);
            }
        }
        Self::new(resolver, scheme, universe, preferences, entries)
    }

    /// The document's own content identity: a digest over its canonical
    /// encoding, identifying one revision of the lock. The rendered file
    /// never feeds a digest (0041); this is what "the same lock" means.
    #[must_use]
    pub fn content_id(&self) -> ContentId {
        value_content_id(LOCK_DOMAIN, &self.to_value())
    }

    /// The lock a resolution writes. The answer contributes the selection
    /// and the universe digest it resolved against; the scheme and the
    /// preference list are request-side inputs the answer deliberately does
    /// not repeat (0040's protocol: an answer names the choice and the
    /// explanation), so the caller — the party holding both halves —
    /// supplies them. Only a solved resolution selects; the other three
    /// constructors are facts about the problem or the run, and a lock
    /// records selections.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the constructor when the
    /// resolution is not `Solved`, and naming the candidate when its
    /// provenance carries no content identity to bind.
    pub fn from_resolution(
        scheme: &str,
        preferences: &PreferenceList,
        resolution: &Resolution,
    ) -> PithResult<Self> {
        let Resolution::Solved {
            choice, universe, ..
        } = resolution
        else {
            return Err(diag(format!(
                "only a solved resolution yields a lock; this one answered {}, which \
                 selects nothing",
                constructor_of(resolution)
            )));
        };
        let mut entries = Vec::with_capacity(choice.len());
        for candidate in choice.iter() {
            entries.push(entry_of(candidate)?);
        }
        Self::new(
            crate::resolve::resolver_revision_hex(),
            scheme,
            *universe,
            preferences.clone(),
            entries,
        )
    }

    /// The entries as pin constraints: exact ranges over the coordinates
    /// each entry carries, attributed to the lock. Fed to an ordinary
    /// resolution as its constraint set, a pinned re-resolution under the
    /// same universe reproduces the selection.
    #[must_use]
    pub fn pins(&self) -> Box<[Constraint]> {
        self.entries
            .iter()
            .map(|entry| Constraint {
                subject: entry.package.identity().clone(),
                range: Range::Exactly(entry.package.version().into()),
                features: entry.features.clone(),
                attribution: format!(
                    "lock {}/{}",
                    entry.package.identity().domain().as_str(),
                    entry.package.identity().name(),
                )
                .into(),
            })
            .collect()
    }
}

fn constructor_of(resolution: &Resolution) -> &'static str {
    match resolution {
        Resolution::Solved { .. } => "solved",
        Resolution::Unsatisfiable { .. } => "unsatisfiable",
        Resolution::Underdetermined { .. } => "underdetermined",
        Resolution::BudgetExhausted { .. } => "budget-exhausted",
    }
}

/// One candidate as one entry. The content side comes from the candidate's
/// provenance where provenance is content: a path records the path it was
/// read at and the content found there, an archive carries the digest the
/// registry's index claimed — which the fetch verifies against bytes — and
/// a git tree a fetch materialized carries the content measured from the
/// bytes that fetch read (0044). A bare git reference still refuses: a
/// revision with a tree hash is content nobody read, and binding
/// coordinates to it would be a claim, not a measurement (0014). The
/// origin is where the candidate was read, a fact of the universe rather
/// than something the provenance can spell: an archive's digest names its
/// content, not the registry that served the claim.
fn entry_of(candidate: &Candidate) -> PithResult<LockEntry> {
    let source = match &candidate.provenance {
        SourceBinding::Path { content, .. } => *content,
        SourceBinding::Archive { archive } => *archive,
        SourceBinding::GitTree { content, .. } => *content,
        SourceBinding::Git { .. } => {
            return Err(diag(format!(
                "the candidate for `{}` version {} carries a git reference, and a revision \
                 with a tree hash is a reference rather than content: there is no content \
                 identity to bind until a fetch materializes the tree",
                candidate.identity.name(),
                candidate.version,
            )));
        }
    };
    Ok(LockEntry::new(
        PackageVersion::new(candidate.identity.clone(), candidate.version.clone()),
        candidate.features.clone(),
        source,
        candidate.origin.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{DomainIdentity, NUMERIC_SEGMENTS, PackageIdentity};
    use crate::lock::Origin;
    use crate::preference::Preference;
    use crate::resolution::TrailEntry;
    use pith_ids::ContentId;

    fn identity(name: &str) -> PackageIdentity {
        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
    }

    fn feats(items: &[&str]) -> Box<[Box<str>]> {
        items.iter().map(|item| (*item).into()).collect()
    }

    fn candidate(name: &str, version: &str, features: &[&str]) -> Candidate {
        Candidate {
            identity: identity(name),
            version: version.into(),
            features: feats(features),
            provenance: SourceBinding::Archive {
                archive: ContentId::of_blob(format!("{name}-{version}").as_bytes()),
            },
            origin: Origin::Registry("pkgs.pith-lang.org".into()),
            requires: Box::new([]),
        }
    }

    fn solved(candidates: &[Candidate]) -> Resolution {
        Resolution::Solved {
            choice: candidates.iter().cloned().collect(),
            trail: candidates
                .iter()
                .map(|c| TrailEntry {
                    subject: c.identity.clone(),
                    considered: 1,
                    decided_by: "sole-candidate".into(),
                })
                .collect(),
            universe: crate::universe::CandidateUniverse::new(candidates.to_vec()).content_id(),
        }
    }

    fn lock_over(candidates: &[Candidate]) -> Lock {
        Lock::from_resolution(
            NUMERIC_SEGMENTS,
            &PreferenceList(Box::new([])),
            &solved(candidates),
        )
        .unwrap()
    }

    #[test]
    fn a_resolution_produces_a_document_with_every_recorded_input() {
        let candidates = [
            candidate("zlib", "1.3", &[]),
            candidate("openssl", "1.1.1", &["shared"]),
        ];
        let lock = lock_over(&candidates);
        assert_eq!(lock.resolver, crate::resolve::resolver_revision_hex());
        assert_eq!(lock.scheme, Box::from(NUMERIC_SEGMENTS));
        assert_eq!(
            lock.universe,
            crate::universe::CandidateUniverse::new(candidates.to_vec()).content_id()
        );
        assert_eq!(lock.entries.len(), 2);
        let openssl = lock
            .entries
            .iter()
            .find(|e| e.package.identity().name() == "openssl")
            .unwrap();
        assert_eq!(openssl.features, feats(&["shared"]));
    }

    #[test]
    fn the_document_has_one_spelling_regardless_of_choice_order() {
        let candidates = [
            candidate("zlib", "1.3", &[]),
            candidate("openssl", "1.1.1", &[]),
        ];
        let mut flipped = candidates.clone();
        flipped.reverse();
        let one = lock_over(&candidates);
        let other = lock_over(&flipped);
        assert_eq!(one, other);
        assert_eq!(one.content_id(), other.content_id());
        assert_eq!(one.to_value(), other.to_value());
    }

    #[test]
    fn only_a_solved_resolution_yields_a_document() {
        let refusal = Resolution::Underdetermined {
            subject: identity("zlib"),
            tied: Box::new([candidate("zlib", "1.3", &[])]),
            orderings: PreferenceList(Box::new([Preference::Newest])),
        };
        let error =
            Lock::from_resolution(NUMERIC_SEGMENTS, &PreferenceList(Box::new([])), &refusal)
                .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("underdetermined")),
            "the diagnostic names the constructor: {error:?}"
        );
    }

    #[test]
    fn a_materialized_git_tree_binds_the_content_the_fetch_measured() {
        let mut git = candidate("zlib", "1.3", &[]);
        git.provenance = SourceBinding::GitTree {
            revision: "9f11b1d".into(),
            tree: "e3b0c44".into(),
            content: ContentId::of_blob(b"zlib-tree-archive"),
        };
        git.origin = Origin::Forge("git.pith-lang.org/pith".into());
        let lock = Lock::from_resolution(
            NUMERIC_SEGMENTS,
            &PreferenceList(Box::new([])),
            &solved(&[git]),
        )
        .unwrap();
        let entry = lock.entries.first().unwrap();
        assert_eq!(entry.source, ContentId::of_blob(b"zlib-tree-archive"));
        assert_eq!(entry.origin, Origin::Forge("git.pith-lang.org/pith".into()));
    }

    #[test]
    fn a_git_candidate_refuses_rather_than_binding_a_reference() {
        let mut git = candidate("zlib", "1.3", &[]);
        git.provenance = SourceBinding::Git {
            revision: "9f11b1d".into(),
            tree: "e3b0c44".into(),
        };
        let error = Lock::from_resolution(
            NUMERIC_SEGMENTS,
            &PreferenceList(Box::new([])),
            &solved(&[git]),
        )
        .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("git") && d.message.0.contains("zlib")),
            "the diagnostic names the candidate and the refusal's ground: {error:?}"
        );
    }

    #[test]
    fn pins_are_exact_constraints_over_the_entry_coordinates() {
        let lock = lock_over(&[candidate("openssl", "1.1.1", &["shared"])]);
        let pins = lock.pins();
        assert_eq!(pins.len(), 1);
        let pin = pins.first().unwrap();
        assert_eq!(pin.subject, identity("openssl"));
        assert_eq!(pin.range, Range::Exactly("1.1.1".into()));
        assert_eq!(pin.features, feats(&["shared"]));
        assert_eq!(pin.attribution.as_ref(), "lock pithpkgs/openssl");
    }

    #[test]
    fn a_moved_header_input_is_named_by_the_diff() {
        let base = lock_over(&[candidate("zlib", "1.3", &[])]);
        let moved_universe = Lock::new(
            base.resolver.clone(),
            base.scheme.clone(),
            ContentId::of_blob(b"another-universe"),
            base.preferences.clone(),
            base.entries.to_vec(),
        )
        .unwrap();
        assert_eq!(
            diff(&base, &moved_universe).changes,
            Box::from([LockChange::Universe(
                base.universe,
                ContentId::of_blob(b"another-universe")
            )])
        );

        let moved_resolver = Lock::new(
            Box::from("other"),
            base.scheme.clone(),
            base.universe,
            base.preferences.clone(),
            base.entries.to_vec(),
        )
        .unwrap();
        assert_eq!(
            diff(&base, &moved_resolver).changes,
            Box::from([LockChange::Resolver])
        );

        let moved_preferences = Lock::new(
            base.resolver.clone(),
            base.scheme.clone(),
            base.universe,
            PreferenceList(Box::new([Preference::Newest])),
            base.entries.to_vec(),
        )
        .unwrap();
        assert_eq!(
            diff(&base, &moved_preferences).changes,
            Box::from([LockChange::Preferences])
        );

        let moved_scheme = Lock::new(
            base.resolver.clone(),
            Box::from("debian"),
            base.universe,
            base.preferences.clone(),
            base.entries.to_vec(),
        )
        .unwrap();
        assert_eq!(
            diff(&base, &moved_scheme).changes,
            Box::from([LockChange::Scheme])
        );
    }

    #[test]
    fn entry_changes_are_reported_as_added_removed_upgraded_and_drifted() {
        let entry = |name: &str, version: &str, features: &[&str], source: &[u8]| {
            LockEntry::new(
                PackageVersion::new(identity(name), version),
                features.iter().copied(),
                ContentId::of_blob(source),
                Origin::Registry("pkgs.pith-lang.org".into()),
            )
        };
        let before = Lock::new(
            Box::from("r"),
            NUMERIC_SEGMENTS,
            ContentId::of_blob(b"u"),
            PreferenceList(Box::new([])),
            vec![
                entry("zlib", "1.3", &[], b"zlib-1.3"),
                entry("left", "0.1", &[], b"left-0.1"),
                entry("drifted", "2.0", &[], b"drifted-original"),
            ],
        )
        .unwrap();
        let after = Lock::new(
            Box::from("r"),
            NUMERIC_SEGMENTS,
            ContentId::of_blob(b"u"),
            PreferenceList(Box::new([])),
            vec![
                entry("zlib", "1.4", &[], b"zlib-1.4"),
                entry("openssl", "1.1.1", &[], b"openssl-1.1.1"),
                entry("drifted", "2.0", &[], b"drifted-republished"),
            ],
        )
        .unwrap();
        let changes = diff(&before, &after).changes;
        assert_eq!(
            changes,
            Box::from([
                LockChange::Drifted {
                    package: PackageVersion::new(identity("drifted"), "2.0"),
                    from: ContentId::of_blob(b"drifted-original"),
                    to: ContentId::of_blob(b"drifted-republished"),
                },
                LockChange::Removed(entry("left", "0.1", &[], b"left-0.1")),
                LockChange::Upgraded {
                    package: identity("zlib"),
                    from: "1.3".into(),
                    to: "1.4".into(),
                },
                LockChange::Added(entry("openssl", "1.1.1", &[], b"openssl-1.1.1")),
            ]),
            "an upgrade over new content is one upgrade, not an upgrade plus drift"
        );
    }

    #[test]
    fn a_moved_feature_set_is_one_selection_moving_not_a_removal_and_an_addition() {
        // Keyed on the package identity, the key parse and the solver use: a
        // feature set that moved reads as the same package's selection
        // moving, and a version that moved alongside it still fires the
        // upgrade.
        let entry = |version: &str, features: &[&str], source: &[u8]| {
            LockEntry::new(
                PackageVersion::new(identity("openssl"), version),
                features.iter().copied(),
                ContentId::of_blob(source),
                Origin::Registry("pkgs.pith-lang.org".into()),
            )
        };
        let before = Lock::new(
            Box::from("r"),
            NUMERIC_SEGMENTS,
            ContentId::of_blob(b"u"),
            PreferenceList(Box::new([])),
            vec![entry("1.1.1", &["shared"], b"openssl-shared")],
        )
        .unwrap();
        let after = Lock::new(
            Box::from("r"),
            NUMERIC_SEGMENTS,
            ContentId::of_blob(b"u"),
            PreferenceList(Box::new([])),
            vec![entry("1.1.1", &["static"], b"openssl-static")],
        )
        .unwrap();
        assert_eq!(
            diff(&before, &after).changes,
            Box::from([LockChange::Features {
                package: identity("openssl"),
                from: feats(&["shared"]),
                to: feats(&["static"]),
            }])
        );

        let moved_both = Lock::new(
            Box::from("r"),
            NUMERIC_SEGMENTS,
            ContentId::of_blob(b"u"),
            PreferenceList(Box::new([])),
            vec![entry("1.2.0", &["static"], b"openssl-static")],
        )
        .unwrap();
        assert_eq!(
            diff(&before, &moved_both).changes,
            Box::from([
                LockChange::Upgraded {
                    package: identity("openssl"),
                    from: "1.1.1".into(),
                    to: "1.2.0".into(),
                },
                LockChange::Features {
                    package: identity("openssl"),
                    from: feats(&["shared"]),
                    to: feats(&["static"]),
                },
            ]),
            "the upgrade fires when the features moved alongside the version"
        );
    }

    #[test]
    fn a_document_cannot_bind_one_package_twice() {
        // The set property parse enforces on files is the document's own, so
        // a lock built in memory cannot render a file the reader refuses.
        let error = Lock::new(
            Box::from("r"),
            NUMERIC_SEGMENTS,
            ContentId::of_blob(b"u"),
            PreferenceList(Box::new([])),
            vec![
                LockEntry::new(
                    PackageVersion::new(identity("openssl"), "1.1.1"),
                    ["shared"],
                    ContentId::of_blob(b"openssl-shared"),
                    Origin::Registry("pkgs.pith-lang.org".into()),
                ),
                LockEntry::new(
                    PackageVersion::new(identity("openssl"), "1.1.1"),
                    ["static"],
                    ContentId::of_blob(b"openssl-static"),
                    Origin::Registry("pkgs.pith-lang.org".into()),
                ),
            ],
        )
        .unwrap_err();
        let message = error
            .iter()
            .next()
            .map(|diagnostic| diagnostic.message.0.to_string())
            .unwrap_or_default();
        assert!(
            message.contains("openssl") && message.contains("twice"),
            "the diagnostic names the package and the conflict: {message}"
        );
    }

    #[test]
    fn the_document_round_trips_through_the_canonical_codec() {
        let lock = lock_over(&[candidate("zlib", "1.3", &["shared"])]);
        let value = lock.to_value();
        assert!(value.is_type(&lock_type()));
        let decoded = Value::decode_canonical(&value.encode_canonical()).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(Lock::from_value(&decoded).unwrap(), lock);
    }

    #[test]
    fn the_document_digests_stably_across_the_codec_boundary() {
        let lock = lock_over(&[candidate("zlib", "1.3", &[])]);
        let decoded = Value::decode_canonical(&lock.to_value().encode_canonical()).unwrap();
        let read_back = Lock::from_value(&decoded).unwrap();
        assert_eq!(read_back.content_id(), lock.content_id());
    }
}
