//! Command implementations and their shared execution context.

mod check;
mod explore;
mod fmt;
mod gc;
mod state;
mod store;

pub use check::Check;
pub use explore::Explore;
pub use fmt::Fmt;
pub use gc::Gc;
pub use state::{StateCheck, StateInfo};
pub use store::{StoreAdd, StoreCat, StoreLs, StoreMaterialize};

use std::path::PathBuf;

use pith_query::{Environment, ReadOnly, Roots, Session, Writable};

use crate::exit::Failure;

pub struct Context {
    store: Option<PathBuf>,
    state: Option<PathBuf>,
    environment: Environment,
}

impl Context {
    pub fn new(store: Option<PathBuf>, state: Option<PathBuf>) -> Self {
        Self {
            store,
            state,
            environment: Environment::from_process(),
        }
    }

    fn roots(&self) -> Result<Roots, Failure> {
        Roots::resolve(&self.environment, self.store.clone(), self.state.clone())
            .map_err(|error| Failure::user(error.to_string()))
    }

    fn read_only(&self) -> Result<Session<ReadOnly>, Failure> {
        Ok(Session::open(self.roots()?)?)
    }

    fn writable(&self) -> Result<Session<Writable>, Failure> {
        Ok(Session::open_writable(self.roots()?)?)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OutputKind {
    Records,
    RawBytes,
}

pub enum CommandOutput {
    Records(Vec<pith_output::OutputRecord>),
    RawBytes(Box<[u8]>),
}

impl CommandOutput {
    pub fn empty(kind: OutputKind) -> Self {
        match kind {
            OutputKind::Records => Self::Records(Vec::new()),
            OutputKind::RawBytes => Self::RawBytes(Box::new([])),
        }
    }

    pub const fn kind(&self) -> OutputKind {
        match self {
            Self::Records(_) => OutputKind::Records,
            Self::RawBytes(_) => OutputKind::RawBytes,
        }
    }
}

pub struct Report {
    pub output: CommandOutput,
    pub failure: Option<Failure>,
}

impl Report {
    pub fn of(records: Vec<pith_output::OutputRecord>) -> Self {
        Self {
            output: CommandOutput::Records(records),
            failure: None,
        }
    }

    pub fn raw(bytes: impl Into<Box<[u8]>>) -> Self {
        Self {
            output: CommandOutput::RawBytes(bytes.into()),
            failure: None,
        }
    }

    pub fn refused(records: Vec<pith_output::OutputRecord>, failure: Failure) -> Self {
        Self {
            output: CommandOutput::Records(records),
            failure: Some(failure),
        }
    }
}

pub trait Execute: Sized {
    const LABEL: &'static str;
    const OUTPUT_KIND: OutputKind = OutputKind::Records;

    fn execute(self, context: &mut Context) -> Result<Report, Failure>;
}

pub trait Runnable {
    fn label(&self) -> &'static str;

    fn output_kind(&self) -> OutputKind;

    fn run(self: Box<Self>, context: &mut Context) -> Result<Report, Failure>;
}

impl<T: Execute> Runnable for T {
    fn label(&self) -> &'static str {
        Self::LABEL
    }

    fn output_kind(&self) -> OutputKind {
        Self::OUTPUT_KIND
    }

    fn run(self: Box<Self>, context: &mut Context) -> Result<Report, Failure> {
        (*self).execute(context)
    }
}
