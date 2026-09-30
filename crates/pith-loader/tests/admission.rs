//! One admission mechanism, distinct claims: a refused module source and a
//! refused binary substitution share the `pith_constraint::Refusal`
//! envelope but carry each mechanism's own clause vocabulary, so no check
//! can construct another's clause. Pins T-5.

use phloem::identity::{DomainIdentity, PackageIdentity, PackageVersion};
use phloem::lock::{LockEntry, Origin};
use phloem::substitution::{Admission, AdmittedOrigins, BinaryOffer};
use pith_constraint::Refusal;
use pith_core::Value;
use pith_engine::ExecutionPlatform;
use pith_hir::{ModuleSubject, VersionRange};
use pith_ids::ContentId;
use pith_loader::{
    AdmissionClause, AdmissionRequest, ProjectSource, admit_source, parse_project_file,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// The one refusal type both mechanisms refuse with.
fn renders<C: std::fmt::Display>(refusal: &Refusal<C>) -> String {
    refusal.to_string()
}

/// A module source declaring a subject other than the route names.
fn refused_source() -> Refusal<AdmissionClause> {
    let parsed = parse_project_file(&ProjectSource::new(
        pith_diag::SourceId::from_raw(0),
        "pith.pi",
        "module example/other 1.2\n\ninputs {\n}\n",
    ));
    let manifest = parsed
        .validated()
        .unwrap_or_else(|error| unreachable!("the fixture project parses: {error:?}"));
    let request = AdmissionRequest::new(
        ModuleSubject::parse("example/greeter")
            .unwrap_or_else(|error| unreachable!("a fixed subject spelling parses: {error}")),
        VersionRange::Any,
    );
    match admit_source(&request, &manifest) {
        Ok(admitted) => unreachable!("another subject is refused: {admitted:?}"),
        Err(refusal) => refusal,
    }
}

#[test]
fn a_refused_source_and_a_refused_substitution_share_the_refusal_type() -> TestResult {
    let source_refusal = refused_source();

    let package = PackageVersion::new(
        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "zlib"),
        "1.3",
    );
    let entry = LockEntry::new(
        package.clone(),
        ["shared"],
        ContentId::of_blob(b"zlib-1.3.tar"),
        Origin::Forge("builds.pith-lang.org".into()),
    );
    let platform = ExecutionPlatform {
        operating_system: "linux".into(),
        architecture: "x86_64".into(),
    };
    let toolchain = Value::Text("gcc-13".into());
    let origins = AdmittedOrigins(Box::new([Origin::Forge("builds.pith-lang.org".into())]));
    let admission = Admission {
        entry: &entry,
        platform: &platform,
        toolchain: &toolchain,
        origins: &origins,
    };
    let offer = BinaryOffer::new(
        package,
        ["shared"],
        ContentId::of_blob(b"zlib-1.3.tar"),
        platform.clone(),
        toolchain.clone(),
        ContentId::of_blob(b"zlib-1.3.so"),
        Origin::Forge("builds.pith-lang.org".into()),
    );
    let tampered: &[u8] = b"other bytes entirely";
    let substitution_refusal = phloem::substitution::admit(&admission, &offer, tampered)
        .expect_err("bytes measuring other than the claim refuse");

    let source_text = renders(&source_refusal);
    let substitution_text = renders(&substitution_refusal);
    assert!(
        source_text.contains("example/greeter") && source_text.contains("example/other"),
        "the source refusal names both subjects: {source_text}"
    );
    assert!(
        substitution_text.contains("claims content") && substitution_text.contains("measure"),
        "the substitution refusal names both digests: {substitution_text}"
    );
    assert_ne!(source_text, substitution_text);
    Ok(())
}
