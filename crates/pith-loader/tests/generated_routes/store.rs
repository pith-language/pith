use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use pith_diag::Diag;
use pith_loader::{
    AcquireFailure, AcquiredProject, AcquiredSource, PROJECT_NAME, ProjectStore, Route, Workspace,
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
            Route::Path { path } | Route::Member { path } => Self::Path((*path).into()),
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
    pub project: String,
}

#[derive(Default)]
pub struct Reads {
    pub located: Vec<(Location, Request)>,
    pub projects: Vec<Location>,
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

impl ProjectStore for Store {
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

    fn project(&mut self, location: &Location) -> Result<AcquiredProject, AcquireFailure> {
        self.reads.borrow_mut().projects.push(location.clone());
        self.modules
            .get(location)
            .map(|module| AcquiredProject {
                label: format!("{location}/{PROJECT_NAME}").into(),
                text: module.project.clone().into(),
            })
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("no project at {location}").into(),
            })
    }

    fn include(
        &mut self,
        _location: &Location,
        path: &str,
    ) -> Result<AcquiredSource, AcquireFailure> {
        Ok(AcquiredSource {
            path: path.into(),
            text: String::new().into(),
        })
    }
}
