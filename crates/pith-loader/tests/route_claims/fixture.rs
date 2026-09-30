use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use pith_diag::Diag;
use pith_loader::{
    AcquireFailure, AcquiredProject, AcquiredSource, FrontendCode, PROJECT_NAME, ProjectStore,
    Route, Workspace,
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
            Route::Path { path } => Self::Path((*path).into()),
            Route::Git {
                url,
                revision,
                subpath,
                ..
            } => Self::git(url, revision, *subpath),
            Route::Archive { url, digest } => Self::archive(url, digest),
            Route::Registry(registry) => Self::Registry(registry.locator().into()),
            Route::Member { .. } => return Err(AcquireFailure::Unsupported { kind: route.kind() }),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Read {
    Locate(Request),
    Project(String),
    Include(String),
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
        let root = "module example/root 1\n\ninputs {\n  registry primary = \"registry\" root \
         \"ed25519:A\"\n  registry alias = \"registry\" root \"ed25519:A\"\n  registry rotated = \
         \"registry\" root \"ed25519:B\"\n  registry other = \"other\" root \"ed25519:A\"\n  domain \
         example from primary\n  left = path \"left\"\n  right = path \"right\"\n}\n";
        Self {
            modules: BTreeMap::from([
                ("root".into(), root.into()),
                (
                    "left".into(),
                    format!("module example/left 1\n\ninputs {{\n  dep = {left}\n}}\n"),
                ),
                (
                    "right".into(),
                    format!("module example/right 1\n\ninputs {{\n  dep = {right}\n}}\n"),
                ),
                (
                    "v1".into(),
                    "module example/dep 1\n\ninputs {\n}\n\nnominal Name = Bool\n".into(),
                ),
                (
                    "v2".into(),
                    "module example/dep 2\n\ninputs {\n}\n\nnominal Name = Text\n".into(),
                ),
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

impl ProjectStore for Scenario {
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

    fn project(&mut self, location: &String) -> Result<AcquiredProject, AcquireFailure> {
        self.reads
            .borrow_mut()
            .push(Read::Project(location.clone()));
        self.modules
            .get(location)
            .map(|text| AcquiredProject {
                label: format!("{location}/{PROJECT_NAME}").into(),
                text: text.clone().into(),
            })
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("no project at {location}").into(),
            })
    }

    fn include(&mut self, location: &String, path: &str) -> Result<AcquiredSource, AcquireFailure> {
        self.reads
            .borrow_mut()
            .push(Read::Include(location.clone()));
        Ok(AcquiredSource {
            path: path.into(),
            text: "nominal Name = Bool\n".into(),
        })
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
        assert_eq!(
            labels,
            [
                format!("right/{PROJECT_NAME}").as_str(),
                format!("left/{PROJECT_NAME}").as_str()
            ]
        );
    }

    pub fn assert_read_once(&self, location: &str) {
        assert_eq!(
            self.reads
                .iter()
                .filter(|observed| **observed == Read::Project(location.into()))
                .count(),
            1,
            "the location's project file is read once"
        );
    }

    /// A route that conflicts still reads its target: a locator names
    /// content, so the subject it claims is declared by the project the
    /// route reaches, and reading it is how the conflict is found. What a
    /// conflict must never do is *acquire* the target's includes.
    pub fn assert_not_read(&self, location: &str) {
        assert!(!self.reads.contains(&Read::Include(location.into())));
    }

    pub fn assert_located(&self, request: Request) {
        assert!(self.reads.contains(&Read::Locate(request)));
    }
}
