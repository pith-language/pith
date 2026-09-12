//! The system library: compose files, users, a service, and boot
//! configuration into one immutable Linux artifact.
//!
//! The artifact is one canonical tree published by a single action, because
//! content enters the store only through capture. Everything its identity
//! rests on is decided above the action by pure rules: the merges compose
//! file sets, user tables, and units from declared contributions, and the
//! renders project the merged values into the texts an /etc and a boot entry
//! are made of. Symlinks are real structure here: an immutable /etc is mostly
//! links, so the assembly creates them and capture records them as entries.
//!
//! ```no_run
//! use pith_engine::Engine;
//! use stele::{SteleEngine, discover, types};
//!
//! let mut engine = Engine::new();
//! engine.register_stele();
//! let shell = "/bin/sh";
//! let closure = discover::tools_closure(&[shell]);
//! let tools = types::tools_value(
//!     shell, "/usr/bin/mkdir", "/usr/bin/cat", "/usr/bin/chmod", "/usr/bin/ln",
//!     &closure.iter().map(String::as_str).collect::<Vec<_>>(),
//! );
//! let boot = types::boot_value("pith", "/boot/vmlinuz", "/boot/initrd");
//! let etc = types::etc_contributions(&[(
//!     "base",
//!     types::file_set_value([("etc/hosts", types::FileBody::File {
//!         content: engine.put_blob(b"127.0.0.1 localhost\n")?,
//!         executable: false,
//!     })]),
//! )]);
//! # Ok::<(), pith_store::StoreError>(())
//! ```

pub mod discover;
pub mod merge;
pub mod rules;
pub mod types;

use pith_engine::Engine;

use rules::{
    AssembleAction, ComposeEtc, ComposeSystem, ComposeUnit, ComposeUsers, RenderBoot, RenderPasswd,
    RenderUnit,
};

/// Registration of this domain's rules onto an [`Engine`].
pub trait SteleEngine {
    /// Register the merges, the renders, the assembly action, and the
    /// compose entry that requests them.
    fn register_stele(&mut self);
}

impl SteleEngine for Engine {
    fn register_stele(&mut self) {
        self.register_action_rule(AssembleAction::rule(), AssembleAction);
        self.register_rule(ComposeEtc::rule(), ComposeEtc);
        self.register_rule(ComposeUsers::rule(), ComposeUsers);
        self.register_rule(ComposeUnit::rule(), ComposeUnit);
        self.register_rule(RenderUnit::rule(), RenderUnit);
        self.register_rule(RenderPasswd::rule(), RenderPasswd);
        self.register_rule(RenderBoot::rule(), RenderBoot);
        self.register_rule(ComposeSystem::rule(), ComposeSystem);
    }
}
