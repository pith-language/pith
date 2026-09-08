//! Admission of an acquired module source: acquisition reads bytes, and
//! admission decides whether they are the module the route asked for. The
//! decision runs through the same machinery a binary substitution admits
//! under, so both produce the same refusal type over their own clause
//! vocabularies, and neither checker can raise the other's clause.

use pith_hir::{Manifest, ManifestVersion, ModuleSubject, VersionRange};

/// The clauses of a module-source admission, one per claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clause {
    /// The manifest declares another subject than the one the route named.
    /// A route selects a module; it cannot rename one.
    Subject {
        requested: ModuleSubject,
        declared: ModuleSubject,
    },
    /// The manifest's version falls outside the range the route wrote. A
    /// path dependency carries no range, so this clause holds trivially
    /// there.
    Version {
        range: VersionRange,
        offered: ManifestVersion,
    },
}

impl std::fmt::Display for Clause {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Subject {
                requested,
                declared,
            } => write!(
                formatter,
                "the route names the subject {requested}, and the manifest declares \
                 {declared}: an alias selects, it does not rename",
            ),
            Self::Version { range, offered } => write!(
                formatter,
                "the route admits versions {range}, and the manifest declares {offered}"
            ),
        }
    }
}

/// The module-source admission's refusal, over this mechanism's clauses.
pub type Refusal = pith_constraint::Refusal<Clause>;

/// What a route asked of the module it names.
pub struct Request {
    subject: ModuleSubject,
    range: VersionRange,
}

impl Request {
    #[must_use]
    pub fn new(subject: ModuleSubject, range: VersionRange) -> Self {
        Self { subject, range }
    }

    #[must_use]
    pub fn subject(&self) -> &ModuleSubject {
        &self.subject
    }

    /// The versions the route admits.
    #[must_use]
    pub fn range(&self) -> &VersionRange {
        &self.range
    }
}

/// The evidence a held admission retains: what was admitted, at which
/// version. Recomputed from the acquired manifest on every read, never
/// stored beside the bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    subject: ModuleSubject,
    version: ManifestVersion,
}

impl Admitted {
    #[must_use]
    pub fn subject(&self) -> &ModuleSubject {
        &self.subject
    }

    #[must_use]
    pub fn version(&self) -> &ManifestVersion {
        &self.version
    }
}

/// Whether the manifest declares the subject the request named.
fn subject(request: &Request, manifest: &Manifest) -> Result<(), Clause> {
    differ(
        manifest.subject() == &request.subject,
        Clause::Subject {
            requested: request.subject.clone(),
            declared: manifest.subject().clone(),
        },
    )
}

/// Whether the manifest's version is one the request's range admits.
fn version(request: &Request, manifest: &Manifest) -> Result<(), Clause> {
    differ(
        request.range.satisfies(manifest.version()),
        Clause::Version {
            range: request.range.clone(),
            offered: manifest.version().clone(),
        },
    )
}

fn differ(held: bool, clause: Clause) -> Result<(), Clause> {
    held.then_some(()).ok_or(clause)
}

/// Admits an acquired manifest against what the route asked for.
///
/// # Errors
/// Returns the first clause that refuses the source.
pub fn admit(request: &Request, manifest: &Manifest) -> Result<Admitted, Refusal> {
    pith_constraint::admit([subject(request, manifest), version(request, manifest)])?;
    Ok(Admitted {
        subject: manifest.subject().clone(),
        version: manifest.version().clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(subject: &str, version: &str) -> Manifest {
        crate::source::parse_manifest(&crate::source::ManifestSource::new(
            pith_diag::SourceId::from_raw(0),
            "module.pi",
            format!("module {subject} {version}\n"),
        ))
        .validated()
        .expect("the fixture manifest parses")
    }

    fn any(subject: &str) -> Request {
        Request::new(ModuleSubject::parse(subject).unwrap(), VersionRange::Any)
    }

    #[test]
    fn a_manifest_declaring_the_requested_subject_at_an_admitted_version_is_admitted() {
        let admitted = admit(&any("example/greeter"), &manifest("example/greeter", "1.2"))
            .expect("subject and version agree with the route");
        assert_eq!(
            admitted.subject(),
            &ModuleSubject::parse("example/greeter").unwrap()
        );
        assert_eq!(admitted.version().canonical_spelling(), Box::from("1.2"));
    }

    #[test]
    fn another_subject_refuses_naming_both_regardless_of_version() {
        let refused = admit(&any("example/greeter"), &manifest("example/other", "1.2"))
            .expect_err("the manifest declares another subject");
        assert!(matches!(refused.clause, Clause::Subject { .. }));
        assert!(
            refused.to_string().contains("example/greeter")
                && refused.to_string().contains("example/other"),
            "the refusal names both subjects: {refused}"
        );
    }

    #[test]
    fn a_version_outside_the_written_range_refuses() {
        let request = Request::new(
            ModuleSubject::parse("example/greeter").unwrap(),
            VersionRange::AtLeast(pith_hir::VersionBound {
                version: ManifestVersion::from_segments([2, 0]).unwrap(),
                inclusive: true,
            }),
        );
        let refused = admit(&request, &manifest("example/greeter", "1.2"))
            .expect_err("the declared version is outside the range");
        assert!(matches!(refused.clause, Clause::Version { .. }));
    }

    #[test]
    fn two_spellings_that_compare_equal_are_one_version() {
        let request = Request::new(
            ModuleSubject::parse("example/greeter").unwrap(),
            VersionRange::Exactly(ManifestVersion::from_segments([1, 2]).unwrap()),
        );
        assert!(
            admit(&request, &manifest("example/greeter", "1.2.0")).is_ok(),
            "1.2 and 1.2.0 compare equal, so one admits the other"
        );
    }
}
