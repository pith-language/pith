//! Registration of xylem's rules onto a pith [`Engine`].
//!
//! The build library owns the wiring between its action rules and the pure
//! rules that request them. A caller discovers a [`Toolchain`] and assembles a
//! [`HeaderUniverse`], registers the rules once, and drives the engine with the
//! request constructors in [`crate::types`].

use pith_engine::Engine;

use crate::rules::{
    CompileAction, CompileRule, HeaderDiscoveryAction, HeaderUniverse, LinkAction, LinkRule,
};
use crate::toolchain::Toolchain;

/// Extension methods for registering xylem's build rules on an [`Engine`].
pub trait BuildEngine {
    /// Register the discovery, compile, and link action rules with their pure
    /// entry rules, over `toolchain`. Sources compiled under this registration
    /// may include the headers `universe` offers; which of them a given source
    /// reads is discovered per source, not declared here.
    fn register_xylem(&mut self, toolchain: Toolchain, universe: HeaderUniverse);
}

impl BuildEngine for Engine {
    fn register_xylem(&mut self, toolchain: Toolchain, universe: HeaderUniverse) {
        let discovery = HeaderDiscoveryAction::new(toolchain.clone(), universe.clone());
        let compile = CompileAction::new(toolchain.clone(), universe);
        let link = LinkAction::new(toolchain);
        self.register_action_rule(discovery.rule(), discovery);
        self.register_action_rule(compile.rule(), compile);
        self.register_action_rule(link.rule(), link);
        self.register_rule(CompileRule::rule(), CompileRule);
        self.register_rule(LinkRule::rule(), LinkRule);
    }
}
