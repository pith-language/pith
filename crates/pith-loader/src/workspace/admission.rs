//! Admission of an acquired module source: acquisition reads bytes, and
//! admission decides whether they are the module the route asked for. The
//! decision runs through the same machinery a binary substitution admits
//! under, so both produce the same refusal type over their own clause
//! vocabularies, and neither checker can raise the other's clause.

use pith_hir::{ModuleSubject, ModuleVersion, Project, VersionRange};

/// The clauses of a module-source admission, one per claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clause {
    /// The project declares another subject than the one the route named.
    /// A route selects a module; it cannot rename one.
    Subject {
        requested: ModuleSubject,
        declared: ModuleSubject,
    },
    /// The project's version falls outside the range the route wrote. A
    /// path input carries no range, so this clause holds trivially there.
    Version {
        range: VersionRange,
        offered: ModuleVersion,
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
                "the route names the subject {requested}, and the project declares \
                 {declared}: an input selects, it does not rename",
            ),
            Self::Version { range, offered } => write!(
                formatter,
                "the route admits versions {range}, and the project declares {offered}"
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
/// version. Recomputed from the acquired project on every read, never
/// stored beside the bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    subject: ModuleSubject,
    version: ModuleVersion,
}

impl Admitted {
    #[must_use]
    pub fn subject(&self) -> &ModuleSubject {
        &self.subject
    }

    #[must_use]
    pub fn version(&self) -> &ModuleVersion {
        &self.version
    }
}

/// Whether the manifest declares the subject the request named.
fn subject(request: &Request, project: &Project) -> Result<(), Clause> {
    differ(
        project.subject() == &request.subject,
        Clause::Subject {
            requested: request.subject.clone(),
            declared: project.subject().clone(),
        },
    )
}

/// Whether the manifest's version is one the request's range admits.
fn version(request: &Request, project: &Project) -> Result<(), Clause> {
    differ(
        request.range.satisfies(project.version()),
        Clause::Version {
            range: request.range.clone(),
            offered: project.version().clone(),
        },
    )
}

fn differ(held: bool, clause: Clause) -> Result<(), Clause> {
    held.then_some(()).ok_or(clause)
}

/// Admits an acquired project against what the route asked for.
///
/// # Errors
/// Returns the first clause that refuses the source.
pub fn admit(request: &Request, project: &Project) -> Result<Admitted, Refusal> {
    pith_constraint::admit([subject(request, project), version(request, project)])?;
    Ok(Admitted {
        subject: project.subject().clone(),
        version: project.version().clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(subject: &str, version: &str) -> Project {
        crate::source::parse_project_file(&crate::source::ProjectSource::new(
            pith_diag::SourceId::from_raw(0),
            "pith.pi",
            format!("module {subject} {version}\n\ninputs {{\n}}\n"),
        ))
        .validated()
        .expect("the fixture project parses")
    }

    fn any(subject: &str) -> Request {
        Request::new(ModuleSubject::parse(subject).unwrap(), VersionRange::Any)
    }

    #[test]
    fn a_project_declaring_the_requested_subject_at_an_admitted_version_is_admitted() {
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
                version: ModuleVersion::from_segments([2, 0]).unwrap(),
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
            VersionRange::Exactly(ModuleVersion::from_segments([1, 2]).unwrap()),
        );
        assert!(
            admit(&request, &manifest("example/greeter", "1.2.0")).is_ok(),
            "1.2 and 1.2.0 compare equal, so one admits the other"
        );
    }
}
