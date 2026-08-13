//! Build and dependency transport over the pith kernel.
//!
//! `xylem` is the first-party build library (docs/foundation/name.md): the
//! tissue that carries sources, toolchains, and artifacts through the kernel's
//! typed rule graph. It owns toolchain closure discovery (decision 0030) and
//! declares the compile and link actions a C build needs, over the nominal
//! types decision 0026 names and the action cache 0031 serves from.
//!
//! A caller discovers a [`Toolchain`] before the run (decision 0007 forbids
//! discovery during evaluation), registers xylem's rules on an
//! [`Engine`](pith_engine::Engine), and
//! drives the graph with the request constructors in [`types`].

pub mod build;
pub mod rules;
pub mod toolchain;
pub mod types;

pub use build::{BuildEngine, Header};
pub use rules::{CompileAction, CompileRule, LinkAction, LinkRule};
pub use toolchain::{DiscoveryError, Toolchain};
