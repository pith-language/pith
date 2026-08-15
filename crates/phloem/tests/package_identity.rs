//! Decision 0039's identity-stability claims, checked end to end against the
//! declared model: an identity survives a version bump, a metadata change,
//! and a source move; a rename is a new identity, not a continuation. Each
//! test changes what the description or the coordinates carry and asserts
//! the identity pair did not move.

use phloem::build::PackageBuild;
use phloem::description::Description;
use phloem::identity::{DomainIdentity, PackageIdentity, PackageVersion};
use phloem::source::SourceBinding;
use pith_ids::ContentId;

const DOMAIN: &str = "pithpkgs";

fn zlib(source: SourceBinding, sources: &[&str]) -> Description {
    Description {
        name: "zlib".into(),
        source,
        build: PackageBuild {
            sources: sources.iter().map(|path| (*path).into()).collect(),
            includes: Box::new([]),
        },
    }
}

fn archive_source() -> SourceBinding {
    SourceBinding::Archive {
        archive: ContentId::of_blob(b"zlib-1.3.tar"),
    }
}

#[test]
fn an_identity_survives_a_version_bump_and_a_metadata_change() {
    let identity = PackageIdentity::declare(DomainIdentity::new(DOMAIN), "zlib");
    let before = PackageVersion::new(identity.clone(), "1.3");
    let after = PackageVersion::new(identity.clone(), "1.3.1");

    assert_ne!(before, after, "a version bump changes the coordinates");
    assert_eq!(before.identity(), after.identity());

    // A metadata change moves the description revision, not the package.
    let before_description = zlib(archive_source(), &["zlib-1.3/zlib.c"]);
    let after_description = zlib(archive_source(), &["zlib-1.3/zlib.c", "zlib-1.3/adler32.c"]);
    assert_eq!(
        before_description.name, after_description.name,
        "the declared name is the identity's name"
    );
    assert_ne!(
        before_description.content_id(),
        after_description.content_id(),
        "a changed build declaration is a new description revision"
    );
    assert_eq!(before.identity(), after.identity());
}

#[test]
fn an_identity_survives_a_source_move() {
    // 0039: a source move the domain's resolution survives is the same
    // package. The identity is the declared pair and reads no content, so a
    // binding that moves from a registry archive to a git revision — with
    // the description digest changing, as a revision should — moves nothing
    // at the identity level.
    let identity = PackageIdentity::declare(DomainIdentity::new(DOMAIN), "zlib");
    let from_registry = zlib(archive_source(), &["zlib-1.3/zlib.c"]);
    let from_git = zlib(
        SourceBinding::Git {
            revision: "9f11b1d".into(),
            tree: "e3b0c44".into(),
        },
        &["zlib-1.3/zlib.c"],
    );

    assert_eq!(from_registry.name, from_git.name);
    assert_ne!(
        from_registry.content_id(),
        from_git.content_id(),
        "a moved source is a new description revision"
    );
    assert_eq!(
        *PackageVersion::new(identity, "1.3").identity(),
        PackageIdentity::declare(DomainIdentity::new(DOMAIN), "zlib")
    );
}

#[test]
fn a_rename_is_a_new_identity_not_a_continuation() {
    // 0039: continuity across a rename is an explicit aliasing operation
    // recorded in provenance, never something the system infers. The
    // construction half of that claim is that `PackageIdentity` has nowhere
    // to carry a predecessor; the inequality below is the other half.
    let old = PackageIdentity::declare(DomainIdentity::new(DOMAIN), "zlib");
    let renamed = PackageIdentity::declare(DomainIdentity::new(DOMAIN), "zlib-ng");

    assert_ne!(old, renamed);
    assert_ne!(
        PackageVersion::new(old, "1.3"),
        PackageVersion::new(renamed, "1.3"),
        "same version, different package: a rename does not carry the lock forward"
    );
}
