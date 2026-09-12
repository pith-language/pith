//! Build and dependency transport over the pith kernel.
//!
//! `xylem` is the first-party build library: it carries sources, toolchains,
//! and artifacts through the kernel's typed rule graph. It owns toolchain
//! closure discovery and header dependency discovery, and declares the actions
//! a C build needs over the kernel's nominal types and its action cache.
//!
//! A caller discovers a [`Toolchain`] and assembles a [`HeaderUniverse`]
//! before the run (discovery during evaluation is not allowed), registers
//! xylem's rules on an [`Engine`](pith_engine::Engine), and drives the graph
//! with the request constructors in [`types`].

pub mod build;
pub mod depfile;
pub mod rules;
pub mod toolchain;
pub mod types;

pub use build::BuildEngine;
pub use rules::{
    CompileAction, CompileRule, GenerateAction, GenerateRule, HeaderDiscoveryAction,
    HeaderUniverse, LinkAction, LinkRule, TestAction, TestRule,
};
pub use toolchain::{DiscoveryError, Toolchain, Toolchains};
