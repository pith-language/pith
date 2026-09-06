//! Routing refusals precede adapter effects.

use std::cell::RefCell;
use std::rc::Rc;

use pith_loader::{
    AcquireFailure, AcquiredManifest, AcquiredSource, FrontendCode, ModuleStore, Route, Workspace,
};

#[derive(Clone, Default)]
struct Reads(Rc<RefCell<Vec<String>>>);

struct Store {
    root: String,
    dependency: String,
    reads: Reads,
}

impl ModuleStore for Store {
    type Location = String;

    fn locate(&self, _: &String, route: &Route<'_>) -> Result<String, AcquireFailure> {
        let target = match route {
            Route::Registry(registry) => {
                assert_eq!(registry.name(), "trusted");
                assert_eq!(registry.locator(), "memory:trusted");
                assert_eq!(registry.root_key().material(), "A");
                registry.subject().to_string()
            }
            Route::Path { subject, .. } => subject.to_string(),
            _ => return Err(AcquireFailure::Unsupported { kind: route.kind() }),
        };
        self.reads.0.borrow_mut().push(format!("locate {target}"));
        Ok(target)
    }

    fn manifest(&mut self, location: &String) -> Result<AcquiredManifest, AcquireFailure> {
        self.reads
            .0
            .borrow_mut()
            .push(format!("manifest {location}"));
        Ok(AcquiredManifest {
            label: format!("{location}/module.pi").into(),
            text: if location == "root" {
                self.root.clone()
            } else {
                self.dependency.clone()
            }
            .into(),
        })
    }

    fn sources(&mut self, location: &String) -> Result<Vec<AcquiredSource>, AcquireFailure> {
        self.reads
            .0
            .borrow_mut()
            .push(format!("sources {location}"));
        Ok(vec![AcquiredSource {
            path: "src/main.pi".into(),
            text: "nominal Name = Text\n".into(),
        }])
    }
}

const BINDING: &str = "registry trusted = \"memory:trusted\" root \"ed25519:A\"\n";

fn resolve(clauses: &str, dependency: &str) -> (Result<Workspace, Box<[pith_diag::Diag]>>, Reads) {
    let reads = Reads::default();
    let result = Workspace::resolve(
        Store {
            root: format!("module example/root 1\n{clauses}"),
            dependency: dependency.into(),
            reads: reads.clone(),
        },
        &"root".to_string(),
    );
    (result, reads)
}

fn refused(clauses: &str, code: FrontendCode) -> Box<[pith_diag::Diag]> {
    let (result, reads) = resolve(clauses, "module example/dep 1\n");
    let diagnostics = result
        .err()
        .unwrap_or_else(|| unreachable!("the configuration must refuse"));
    assert_eq!(
        *reads.0.borrow(),
        ["manifest root"],
        "no adapter call follows the root read"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == code.stable()),
        "{diagnostics:?}"
    );
    assert!(diagnostics.iter().all(|diag| {
        diag.source
            .as_ref()
            .is_some_and(|source| source.label.as_ref() == "root/module.pi")
    }));
    diagnostics
}

#[test]
fn duplicate_domains_name_both_clauses_before_member_or_dependency_io() {
    let diagnostics = refused(
        &format!(
            "{BINDING}domain example from trusted\ndomain example from trusted\nworkspace {{ members: [\"member\"] }}\nuse dep = example/dep\n"
        ),
        FrontendCode::DuplicateRegistry,
    );
    let diagnostic = diagnostics
        .iter()
        .find(|diag| diag.code == FrontendCode::DuplicateRegistry.stable())
        .unwrap();
    let note = diagnostic
        .notes
        .first()
        .expect("the first binding is labeled");
    assert_ne!(note.span, diagnostic.span);
}

#[test]
fn an_unknown_registry_in_a_domain_clause_refuses_without_a_dependency() {
    refused(
        "domain example from missing\n",
        FrontendCode::UnroutedDomain,
    );
}

#[test]
fn missing_domains_do_not_fall_back_to_a_configured_registry() {
    let diagnostics = refused(
        &format!(
            "{BINDING}domain other from trusted\nuse local = example/local from path \"local\"\nuse dep = example/dep\n"
        ),
        FrontendCode::UnroutedDomain,
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.0.contains("other from trusted"))
    );
}

#[test]
fn an_unknown_explicit_registry_does_not_fall_back_to_the_domain_route() {
    refused(
        &format!(
            "{BINDING}domain example from trusted\nuse dep = example/dep from registry missing\n"
        ),
        FrontendCode::UnroutedDomain,
    );
}

#[test]
fn domain_and_explicit_routes_deliver_the_consumers_binding() {
    for route in [
        "domain example from trusted\nuse dep = example/dep\n",
        "use dep = example/dep from registry trusted\n",
    ] {
        let (result, _) = resolve(&format!("{BINDING}{route}"), "module example/dep 1\n");
        assert!(result.is_ok(), "{:?}", result.err());
    }
}

#[test]
fn dependency_authority_cannot_trigger_its_own_acquisition() {
    let dependency = format!(
        "module example/dep 1\n{BINDING}domain hostile from trusted\nuse child = hostile/child\n"
    );
    let (result, reads) = resolve("use dep = example/dep from path \"dep\"\n", &dependency);
    let diagnostics = result.err().expect("dependency authority is refused");
    assert!(diagnostics.iter().any(|diag| {
        diag.code == FrontendCode::DependencySuppliedAuthority.stable()
            && diag
                .source
                .as_ref()
                .is_some_and(|source| source.label.as_ref() == "example/dep/module.pi")
    }));
    assert!(
        !reads
            .0
            .borrow()
            .iter()
            .any(|read| read.contains("hostile/child"))
    );
}
