use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use pith_diag::Diag;
use pith_loader::{
    AcquireFailure, AcquiredManifest, AcquiredSource, ModuleStore, Route, Workspace,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Location {
    Root,
    Branch(usize, u8),
    Source(String),
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Request {
    Path(String),
    Git(String, String, Option<String>),
    Archive(String, String),
    Registry(String),
}

impl Request {
    fn of(route: &Route<'_>) -> Self {
        match route {
            Route::Path { path, .. } | Route::Member { path } => Self::Path((*path).into()),
            Route::Git {
                url,
                revision,
                subpath,
                ..
            } => Self::Git((*url).into(), (*revision).into(), subpath.map(Into::into)),
            Route::Archive { url, digest, .. } => Self::Archive((*url).into(), (*digest).into()),
            Route::Registry(registry) => Self::Registry(registry.locator().into()),
        }
    }
}

pub struct Module {
    pub manifest: String,
    pub source: String,
}

#[derive(Default)]
pub struct Reads {
    pub located: Vec<(Location, Request)>,
    pub manifests: Vec<Location>,
    pub sources: Vec<Location>,
}

#[derive(Default)]
pub struct Store {
    pub modules: BTreeMap<Location, Module>,
    pub routes: BTreeMap<(Location, Request), Location>,
    reads: Rc<RefCell<Reads>>,
}

impl Store {
    pub fn run(self) -> (Result<Workspace, Box<[Diag]>>, Reads) {
        let reads = Rc::clone(&self.reads);
        let result = Workspace::resolve(self, &Location::Root);
        (result, reads.take())
    }
}

impl ModuleStore for Store {
    type Location = Location;

    fn locate(&self, base: &Location, route: &Route<'_>) -> Result<Location, AcquireFailure> {
        let key = (base.clone(), Request::of(route));
        self.reads.borrow_mut().located.push(key.clone());
        self.routes
            .get(&key)
            .cloned()
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("unavailable: {base}").into(),
            })
    }

    fn manifest(&mut self, location: &Location) -> Result<AcquiredManifest, AcquireFailure> {
        self.reads.borrow_mut().manifests.push(location.clone());
        self.modules
            .get(location)
            .map(|module| AcquiredManifest {
                label: format!("{location}/module.pi").into(),
                text: module.manifest.clone().into(),
            })
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("no manifest at {location}").into(),
            })
    }

    fn sources(&mut self, location: &Location) -> Result<Vec<AcquiredSource>, AcquireFailure> {
        self.reads.borrow_mut().sources.push(location.clone());
        self.modules
            .get(location)
            .map(|module| {
                vec![AcquiredSource {
                    path: "src/main.pi".into(),
                    text: module.source.clone().into(),
                }]
            })
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("no sources at {location}").into(),
            })
    }
}
