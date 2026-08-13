//! Registration of xylem's rules onto a pith [`Engine`].
//!
//! The build library owns the wiring between its action rules and the pure
//! rules that request them. A caller discovers a [`Toolchain`], registers the
//! rules once, and drives the engine with the request constructors in
//! [`crate::types`].

use pith_engine::Engine;

use crate::rules::{CompileAction, CompileRule, LinkAction, LinkRule};
use crate::toolchain::Toolchain;

/// The header a compile source includes, at the path the source names it. A
/// source that includes nothing passes `None`.
pub type Header = Option<(Box<str>, pith_ids::ContentId)>;

/// Extension methods for registering xylem's build rules on an [`Engine`].
pub trait BuildEngine {
    /// Register the compile and link action rules and their pure entry rules,
    /// over `toolchain`. Sources compiled under this registration may include
    /// `header` (declared so the executor's confinement permits the read).
    fn register_xylem(&mut self, toolchain: Toolchain, header: Header);
}

impl BuildEngine for Engine {
    fn register_xylem(&mut self, toolchain: Toolchain, header: Header) {
        let compile_action = CompileAction::new(toolchain.clone(), header);
        let link_action = LinkAction::new(toolchain);
        self.register_action_rule(compile_action.rule(), compile_action);
        self.register_action_rule(link_action.rule(), link_action);
        self.register_rule(CompileRule::rule(), CompileRule);
        self.register_rule(LinkRule::rule(), LinkRule);
    }
}
