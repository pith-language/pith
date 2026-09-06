//! Route agreement is checked across consumers, before reacquiring a subject.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use pith_loader::{
    AcquireFailure, AcquiredManifest, AcquiredSource, FrontendCode, ModuleStore, Route, Workspace,
};

struct Store {
    root: String,
    modules: BTreeMap<String, String>,
    reads: Rc<RefCell<Vec<String>>>,
}

impl ModuleStore for Store {
    type Location = String;

    fn locate(&self, _: &String, route: &Route<'_>) -> Result<String, AcquireFailure> {
        let location = match route {
            Route::Registry(registry) => registry.subject().to_string(),
            Route::Git { url, .. } | Route::Archive { url, .. } => (*url).to_string(),
            Route::Path { path, .. } => path.strip_prefix("./").unwrap_or(path).to_string(),
            Route::Member { .. } => return Err(AcquireFailure::Unsupported { kind: route.kind() }),
        };
        self.reads.borrow_mut().push(format!("locate {location}"));
        Ok(location)
    }

    fn manifest(&mut self, location: &String) -> Result<AcquiredManifest, AcquireFailure> {
        self.reads.borrow_mut().push(format!("manifest {location}"));
        let text = if location == "root" {
            &self.root
        } else {
            self.modules
                .get(location)
                .ok_or_else(|| AcquireFailure::Unreadable {
                    message: format!("unknown location {location}").into(),
                })?
        };
        Ok(AcquiredManifest {
            label: format!("{location}/module.pi").into(),
            text: text.clone().into(),
        })
    }

    fn sources(&mut self, _: &String) -> Result<Vec<AcquiredSource>, AcquireFailure> {
        Ok(vec![AcquiredSource {
            path: "src/main.pi".into(),
            text: "nominal Name = Text\n".into(),
        }])
    }
}

fn diamond(left: &str, right: &str) -> (Result<Workspace, Box<[pith_diag::Diag]>>, Vec<String>) {
    let reads = Rc::new(RefCell::new(Vec::new()));
    let modules = BTreeMap::from([
        (
            "left".into(),
            format!("module example/left 1\nuse dep = example/dep {left}\n"),
        ),
        (
            "right".into(),
            format!("module example/right 1\nuse dep = example/dep {right}\n"),
        ),
        ("first".into(), "module example/dep 1\n".into()),
        ("second".into(), "module example/dep 1\n".into()),
        ("example/dep".into(), "module example/dep 1\n".into()),
    ]);
    let root = "module example/root 1\nregistry registry = \"memory\" root \"ed25519:A\"\ndomain example from registry\nuse left = example/left from path \"left\"\nuse right = example/right from path \"right\"\n".into();
    let result = Workspace::resolve(
        Store {
            root,
            modules,
            reads: Rc::clone(&reads),
        },
        &"root".to_string(),
    );
    (result, reads.take())
}

#[test]
fn a_conflicting_revision_names_both_consumers_before_the_second_adapter_call() {
    for (left, right) in [("one", "two"), ("two", "one")] {
        let (result, reads) = diamond(
            &format!("from git \"first\" revision \"{left}\""),
            &format!("from git \"second\" revision \"{right}\""),
        );
        let diagnostics = result
            .err()
            .expect("two revisions cannot satisfy one subject");
        let labels: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == FrontendCode::ConflictingRoutes.stable())
            .filter_map(|diag| diag.source.as_ref().map(|source| source.label.as_ref()))
            .collect();
        assert_eq!(labels, ["right/module.pi", "left/module.pi"]);
        assert!(!reads.iter().any(|read| read == "locate second"));
    }
}

#[test]
fn mirrors_of_one_revision_reuse_the_first_acquired_location() {
    let (result, reads) = diamond(
        "from git \"first\" revision \"same\"",
        "from git \"second\" revision \"same\"",
    );
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(
        reads
            .iter()
            .filter(|read| *read == "manifest first")
            .count(),
        1
    );
    assert!(!reads.iter().any(|read| read.contains("second")));
}

#[test]
fn git_subpaths_are_part_of_the_claim() {
    let (result, _) = diamond(
        "from git \"first\" revision \"same\" subpath \"one\"",
        "from git \"second\" revision \"same\" subpath \"two\"",
    );
    assert!(
        result
            .err()
            .expect("different subpaths differ")
            .iter()
            .any(|diag| diag.code == FrontendCode::ConflictingRoutes.stable())
    );
}

#[test]
fn canonical_path_agreement_loads_once_and_different_paths_refuse() {
    let (result, reads) = diamond("from path \"first\"", "from path \"./first\"");
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(
        reads
            .iter()
            .filter(|read| *read == "manifest first")
            .count(),
        1
    );
    let (result, reads) = diamond("from path \"first\"", "from path \"second\"");
    assert!(
        result
            .err()
            .expect("different canonical paths conflict")
            .iter()
            .any(|diag| diag.code == FrontendCode::ConflictingRoutes.stable())
    );
    assert!(!reads.iter().any(|read| read == "manifest second"));
}

#[test]
fn source_kinds_cannot_silently_replace_one_another() {
    let (result, _) = diamond(
        "from path \"first\"",
        "from git \"second\" revision \"same\"",
    );
    assert!(
        result
            .err()
            .expect("a path and an immutable route conflict")
            .iter()
            .any(|diag| diag.code == FrontendCode::ConflictingRoutes.stable())
    );
}

#[test]
fn a_loaded_registry_selection_is_checked_against_every_consumers_range() {
    let (result, reads) = diamond(">= 1", ">= 2");
    assert!(
        result
            .err()
            .expect("the second consumer excludes version 1")
            .iter()
            .any(|diag| diag.code == FrontendCode::UnexpectedSubject.stable()
                && diag
                    .source
                    .as_ref()
                    .is_some_and(|source| source.label.as_ref() == "right/module.pi"))
    );
    assert_eq!(
        reads
            .iter()
            .filter(|read| *read == "manifest example/dep")
            .count(),
        1
    );
}

#[test]
fn archive_integrity_claims_agree_across_locators_and_refuse_changed_digests() {
    let (result, reads) = diamond(
        "from archive \"first\" digest \"same\"",
        "from archive \"second\" digest \"same\"",
    );
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(!reads.iter().any(|read| read.contains("second")));
    let (result, _) = diamond(
        "from archive \"first\" digest \"one\"",
        "from archive \"second\" digest \"two\"",
    );
    assert!(
        result
            .err()
            .expect("different digests conflict")
            .iter()
            .any(|diag| diag.code == FrontendCode::ConflictingRoutes.stable())
    );
}
