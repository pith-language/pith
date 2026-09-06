use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use pith_diag::Diag;
use pith_loader::{
    AcquireFailure, AcquiredManifest, AcquiredSource, FrontendCode, ModuleStore, Route, Workspace,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Request {
    Path(String),
    Git {
        url: String,
        revision: String,
        subpath: Option<String>,
    },
    Archive {
        url: String,
        digest: String,
    },
    Registry(String),
}

impl Request {
    pub fn git(url: &str, revision: &str, subpath: Option<&str>) -> Self {
        Self::Git {
            url: url.into(),
            revision: revision.into(),
            subpath: subpath.map(Into::into),
        }
    }

    pub fn archive(url: &str, digest: &str) -> Self {
        Self::Archive {
            url: url.into(),
            digest: digest.into(),
        }
    }

    fn of(route: &Route<'_>) -> Result<Self, AcquireFailure> {
        Ok(match route {
            Route::Path { path, .. } => Self::Path((*path).into()),
            Route::Git {
                url,
                revision,
                subpath,
                ..
            } => Self::git(url, revision, *subpath),
            Route::Archive { url, digest, .. } => Self::archive(url, digest),
            Route::Registry(registry) => Self::Registry(registry.locator().into()),
            Route::Member { .. } => return Err(AcquireFailure::Unsupported { kind: route.kind() }),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Read {
    Locate(Request),
    Manifest(String),
    Sources(String),
}

pub struct Scenario {
    modules: BTreeMap<String, String>,
    locations: BTreeMap<Request, String>,
    reads: Rc<RefCell<Vec<Read>>>,
}

pub struct Outcome {
    pub result: Result<Workspace, Box<[Diag]>>,
    pub reads: Vec<Read>,
}

impl Scenario {
    pub fn diamond(left: &str, right: &str) -> Self {
        let root = "module example/root 1\nregistry primary = \"registry\" root \"ed25519:A\"\nregistry alias = \"registry\" root \"ed25519:A\"\nregistry rotated = \"registry\" root \"ed25519:B\"\nregistry other = \"other\" root \"ed25519:A\"\ndomain example from primary\nuse left = example/left from path \"left\"\nuse right = example/right from path \"right\"\n";
        Self {
            modules: BTreeMap::from([
                ("root".into(), root.into()),
                (
                    "left".into(),
                    format!("module example/left 1\nuse dep = example/dep {left}\n"),
                ),
                (
                    "right".into(),
                    format!("module example/right 1\nuse dep = example/dep {right}\n"),
                ),
                ("v1".into(), "module example/dep 1\n".into()),
                ("v2".into(), "module example/dep 2\n".into()),
            ]),
            locations: BTreeMap::from([
                (Request::Path("left".into()), "left".into()),
                (Request::Path("right".into()), "right".into()),
            ]),
            reads: Rc::default(),
        }
    }

    pub fn bind(mut self, request: Request, location: &str) -> Self {
        self.locations.insert(request, location.into());
        self
    }

    pub fn run(self) -> Outcome {
        let reads = Rc::clone(&self.reads);
        let result = Workspace::resolve(self, &"root".to_string());
        Outcome {
            result,
            reads: reads.take(),
        }
    }
}

impl ModuleStore for Scenario {
    type Location = String;

    fn locate(&self, _: &String, route: &Route<'_>) -> Result<String, AcquireFailure> {
        let request = Request::of(route)?;
        self.reads.borrow_mut().push(Read::Locate(request.clone()));
        self.locations
            .get(&request)
            .cloned()
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("adapter refuses {request:?}").into(),
            })
    }

    fn manifest(&mut self, location: &String) -> Result<AcquiredManifest, AcquireFailure> {
        self.reads
            .borrow_mut()
            .push(Read::Manifest(location.clone()));
        self.modules
            .get(location)
            .map(|text| AcquiredManifest {
                label: format!("{location}/module.pi").into(),
                text: text.clone().into(),
            })
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("no manifest at {location}").into(),
            })
    }

    fn sources(&mut self, location: &String) -> Result<Vec<AcquiredSource>, AcquireFailure> {
        self.reads
            .borrow_mut()
            .push(Read::Sources(location.clone()));
        let body = if location == "v2" { "Text" } else { "Bool" };
        Ok(vec![AcquiredSource {
            path: "src/main.pi".into(),
            text: format!("nominal Name = {body}\n").into(),
        }])
    }
}

impl Outcome {
    pub fn assert_conflict(&self) {
        let diagnostics = self
            .result
            .as_ref()
            .err()
            .unwrap_or_else(|| unreachable!("distinct sources must refuse"));
        let labels: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == FrontendCode::ConflictingRoutes.stable())
            .filter_map(|diag| diag.source.as_ref().map(|source| source.label.as_ref()))
            .collect();
        assert_eq!(labels, ["right/module.pi", "left/module.pi"]);
    }

    pub fn assert_read_once(&self, location: &str) {
        for read in [
            Read::Manifest(location.into()),
            Read::Sources(location.into()),
        ] {
            assert_eq!(
                self.reads
                    .iter()
                    .filter(|observed| *observed == &read)
                    .count(),
                1,
                "{read:?}"
            );
        }
    }

    pub fn assert_not_read(&self, location: &str) {
        assert!(!self.reads.contains(&Read::Manifest(location.into())));
    }

    pub fn assert_located(&self, request: Request) {
        assert!(self.reads.contains(&Read::Locate(request)));
    }
}
