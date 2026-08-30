//! A domain library the kernel does not know about.
//!
//! Requirement U-10 says each first-party domain uses public kernel interfaces
//! and that "tests prove that an external library can replace or extend it
//! without hidden hooks". Until this crate there was no such test: xylem and
//! phloem are the evidence for the interface being usable, and phloem depends
//! on xylem, so the workspace held one independent domain and one layered on
//! it. This crate is the second independent one, and the proof is why it
//! exists.
//!
//! It renders a text template: a pure entry checks that every placeholder the
//! template spells is bound, and an action runs a renderer program over the
//! template with the bindings as arguments. Since M-13 the whole module is
//! authored in `example.pi` — declarations, the action rule's signature, and
//! the entry's represented body — and the crate binds its one host body to a
//! coordinate it does not own: the loader elaborates the file, the entry
//! registers as represented data, and the action binds through
//! [`pith_loader::HostRuleDeclaration::bind`]. What the crate demonstrates is that
//! declared types, represented bodies, and one bound host body are ordinary
//! public API, and that reuse, hydration, and contract inspection follow.
//! Membership in the first-party set adds nothing.
//!
//! ```no_run
//! use example_domain::{ExampleEngine, types};
//! use pith_engine::Engine;
//!
//! let mut engine = Engine::new();
//! engine.register_example_domain();
//! let renderer = engine.put_blob(b"#!/bin/sh\n")?;
//! let template = engine.put_blob(b"hello {{name}}\n")?;
//! let request = types::render_request(
//!     renderer,
//!     template,
//!     types::bindings_value([("name", "world")]),
//! );
//! # Ok::<(), pith_store::StoreError>(())
//! ```

pub mod rules;
pub mod types;

use pith_diag::SourceId;
use pith_engine::Engine;
use pith_loader::{ImportEnv, ModuleSource, load_module};

pub use rules::RenderAction;

/// Registration of this domain onto an [`Engine`].
///
/// An extension trait over the engine, which is how a domain that the engine
/// does not depend on adds a method to it. xylem's `BuildEngine` is the same
/// shape, and that it is available to a crate outside the first-party set is
/// part of what this domain is here to show. The rules themselves come from
/// `example.pi`: the entry's body is represented data, and the action is this
/// crate's host body bound to the coordinate the file declares.
pub trait ExampleEngine {
    /// Load the module and register its rules.
    fn register_example_domain(&mut self);
}

impl ExampleEngine for Engine {
    fn register_example_domain(&mut self) {
        let source = ModuleSource::new(
            types::MODULE,
            SourceId::from_raw(0),
            "example.pi",
            include_str!("../example.pi"),
        );
        let loaded = match load_module(&source, &ImportEnv::new()) {
            Ok(loaded) => loaded,
            Err(diagnostics) => unreachable!("example.pi does not elaborate: {diagnostics:?}"),
        };
        let action = loaded
            .action_rule(types::RENDER)
            .unwrap_or_else(|| unreachable!("example.pi declares no action rule `render`"));
        action.bind(self, pith_core::BodyRevision(1), RenderAction);
        for rule in loaded.represented_pure_rules() {
            match rule.register(self) {
                Ok(_) => {}
                Err(error) => {
                    unreachable!("example.pi's represented bodies do not register: {error}")
                }
            }
        }
    }
}
