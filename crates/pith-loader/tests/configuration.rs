//! Effective authority retains its owner through configuration precedence.

use std::cell::RefCell;
use std::rc::Rc;

use pith_diag::{Diag, SourceId};
use pith_loader::{
    AcquireFailure, AcquiredManifest, AcquiredSource, BindingOrigin, BindingPolicy, FrontendCode,
    ManifestSource, ModuleStore, Route, UserBindings, Workspace, WorkspaceResolution,
    parse_manifest,
};

type ResultOf<T> = Result<T, Box<[Diag]>>;

#[derive(Debug, PartialEq, Eq)]
struct Observed {
    locator: String,
    origin: BindingOrigin,
}

struct Store {
    root: String,
    observed: Rc<RefCell<Vec<Observed>>>,
}

impl ModuleStore for Store {
    type Location = String;

    fn locate(&self, _: &String, route: &Route<'_>) -> Result<String, AcquireFailure> {
        let Route::Registry(registry) = route else {
            return Err(AcquireFailure::Unsupported { kind: route.kind() });
        };
        self.observed.borrow_mut().push(Observed {
            locator: registry.locator().into(),
            origin: registry.registry_site().origin(),
        });
        Ok(registry.subject().to_string())
    }

    fn manifest(&mut self, location: &String) -> Result<AcquiredManifest, AcquireFailure> {
        Ok(AcquiredManifest {
            label: format!("{location}/module.pi").into(),
            text: if location == "root" {
                self.root.clone()
            } else {
                format!("module {location} 1\n")
            }
            .into(),
        })
    }

    fn sources(&mut self, _: &String) -> Result<Vec<AcquiredSource>, AcquireFailure> {
        Ok(vec![AcquiredSource {
            path: "src/main.pi".into(),
            text: "nominal Name = Text\n".into(),
        }])
    }
}

fn user(clauses: &str) -> ResultOf<UserBindings> {
    UserBindings::try_from(&parse_manifest(&ManifestSource::new(
        SourceId::from_raw(0),
        "user.pi",
        format!("module config/user 1\n{clauses}"),
    )))
}

fn binding(locator: &str) -> String {
    format!("registry registry = \"{locator}\" root \"ed25519:A\"\n")
}

fn resolve(
    project: &str,
    user_clauses: &str,
    policy: BindingPolicy,
) -> (ResultOf<WorkspaceResolution>, Vec<Observed>) {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let result = user(user_clauses).and_then(|user| {
        Workspace::resolve_configured(
            Store {
                root: format!("module example/root 1\n{project}"),
                observed: Rc::clone(&observed),
            },
            &"root".to_string(),
            Some(user),
            policy,
        )
    });
    let reads = observed.take();
    (result, reads)
}

#[test]
fn user_authority_supplies_an_unbound_domain() {
    let (result, observed) = resolve(
        "use dep = example/dep\n",
        &format!("{}domain example from registry\n", binding("user")),
        BindingPolicy::AllowUser,
    );
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(
        observed,
        [Observed {
            locator: "user".into(),
            origin: BindingOrigin::User
        }]
    );
}

#[test]
fn project_override_retains_both_sources_and_warns() {
    let (result, observed) = resolve(
        &format!(
            "{}domain example from registry\nuse dep = example/dep\n",
            binding("project")
        ),
        &format!("{}domain example from registry\n", binding("user")),
        BindingPolicy::AllowUser,
    );
    let resolved = result.expect("project precedence succeeds");
    assert_eq!(
        observed,
        [Observed {
            locator: "project".into(),
            origin: BindingOrigin::Project
        }]
    );
    assert_eq!(resolved.overrides().len(), 2);
    for collision in resolved.overrides() {
        assert_eq!(
            collision.project().source().label.as_ref(),
            "root/module.pi"
        );
        assert_eq!(collision.user().source().label.as_ref(), "user.pi");
        assert_ne!(
            collision.project().source().id,
            collision.user().source().id
        );
        assert!(
            collision
                .diagnostics()
                .iter()
                .all(|diag| diag.severity == pith_diag::Severity::Warning)
        );
    }
}

#[test]
fn a_project_registry_name_cannot_capture_a_user_domain_route() {
    let (result, observed) = resolve(
        &format!("{}use dep = example/dep\n", binding("project")),
        &format!("{}domain example from registry\n", binding("user")),
        BindingPolicy::AllowUser,
    );
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(
        observed,
        [Observed {
            locator: "user".into(),
            origin: BindingOrigin::User
        }]
    );
}

#[test]
fn project_only_authority_refuses_user_domain_and_explicit_routes_before_locate() {
    for requirement in [
        "use dep = example/dep\n",
        "use dep = example/dep from registry registry\n",
    ] {
        let (result, observed) = resolve(
            requirement,
            &format!("{}domain example from registry\n", binding("user")),
            BindingPolicy::ProjectOnly,
        );
        let diagnostics = result.err().expect("user authority is refused");
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == FrontendCode::ModeExceeded.stable())
        );
        assert!(observed.is_empty());
    }
}

#[test]
fn project_owned_selection_is_allowed_with_unused_user_configuration() {
    let (result, _) = resolve(
        &format!(
            "{}domain example from registry\nuse dep = example/dep\n",
            binding("project")
        ),
        &format!("{}domain other from registry\n", binding("user")),
        BindingPolicy::ProjectOnly,
    );
    assert!(result.is_ok(), "{:?}", result.err());
}

#[test]
fn invalid_user_configuration_is_not_hidden_by_a_project_override() {
    let (result, observed) = resolve(
        &format!(
            "{}domain example from registry\nuse dep = example/dep\n",
            binding("project")
        ),
        &format!(
            "{}domain example from registry\ndomain example from registry\n",
            binding("user")
        ),
        BindingPolicy::AllowUser,
    );
    let diagnostics = result
        .err()
        .expect("duplicate domains are invalid in either layer");
    assert!(diagnostics.iter().any(|diag| {
        diag.code == FrontendCode::DuplicateRegistry.stable()
            && diag
                .source
                .as_ref()
                .is_some_and(|source| source.label.as_ref() == "user.pi")
    }));
    assert!(observed.is_empty());
}

#[test]
fn a_project_domain_cannot_silently_inherit_a_user_registry() {
    let (result, observed) = resolve(
        "domain example from registry\nuse dep = example/dep\n",
        &binding("user"),
        BindingPolicy::AllowUser,
    );
    assert!(
        result
            .err()
            .expect("each layer binds its own names")
            .iter()
            .any(|diag| diag.code == FrontendCode::UnroutedDomain.stable())
    );
    assert!(observed.is_empty());
}
