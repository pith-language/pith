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
//! template with the bindings as arguments. Its declarations are its own
//! (decision 0047), its rules register through
//! [`Engine::register_rule`](pith_engine::Engine::register_rule) and
//! [`register_action_rule`](pith_engine::Engine::register_action_rule), and it
//! depends on neither xylem nor phloem. What it demonstrates is that the two
//! things a domain needs — declared types and registered rules — are ordinary
//! public API, and that reuse, hydration, and contract inspection follow from
//! those two calls. Membership in the first-party set adds nothing.
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

use pith_engine::Engine;

pub use rules::{RenderAction, RenderRule};

/// Registration of this domain's rules onto an [`Engine`].
///
/// An extension trait over the engine, which is how a domain that the engine
/// does not depend on adds a method to it. xylem's `BuildEngine` is the same
/// shape, and that it is available to a crate outside the first-party set is
/// part of what this domain is here to show.
pub trait ExampleEngine {
    /// Register the render action and the pure entry that requests it.
    fn register_example_domain(&mut self);
}

impl ExampleEngine for Engine {
    fn register_example_domain(&mut self) {
        self.register_action_rule(RenderAction::rule(), RenderAction);
        self.register_rule(RenderRule::rule(), RenderRule);
    }
}
