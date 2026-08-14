//! Registration of xylem's rules onto a pith [`Engine`].
//!
//! The build library owns the wiring between its action rules and the pure
//! rules that request them. A caller discovers its [`Toolchains`] and assembles
//! a [`HeaderUniverse`], registers the rules once, and drives the engine with
//! the request constructors in [`crate::types`].

use pith_engine::Engine;

use crate::rules::{
    CompileAction, CompileRule, GenerateAction, GenerateRule, HeaderDiscoveryAction,
    HeaderUniverse, LinkAction, LinkRule, TestAction, TestRule,
};
use crate::toolchain::Toolchains;

/// Extension methods for registering xylem's build rules on an [`Engine`].
pub trait BuildEngine {
    /// Register every action rule with its pure entry rule, over `toolchains`.
    ///
    /// One registration serves every toolchain in the set: a request names the
    /// driver it wants and the rule resolves that toolchain's closure, so two
    /// compilers share one graph. Registering per toolchain would give two rules
    /// one interface and collide as `E-1102`.
    ///
    /// Sources compiled under this registration may include the headers
    /// `universe` offers; which of them a given source reads is discovered per
    /// source, not declared here.
    fn register_xylem(&mut self, toolchains: Toolchains, universe: HeaderUniverse);
}

impl BuildEngine for Engine {
    fn register_xylem(&mut self, toolchains: Toolchains, universe: HeaderUniverse) {
        let discovery = HeaderDiscoveryAction::new(toolchains.clone(), universe.clone());
        let compile = CompileAction::new(toolchains.clone(), universe);
        let link = LinkAction::new(toolchains.clone());
        let generate = GenerateAction::new(toolchains.clone());
        let test = TestAction::new(toolchains);
        self.register_action_rule(discovery.rule(), discovery);
        self.register_action_rule(compile.rule(), compile);
        self.register_action_rule(link.rule(), link);
        self.register_action_rule(generate.rule(), generate);
        self.register_action_rule(test.rule(), test);
        self.register_rule(CompileRule::rule(), CompileRule);
        self.register_rule(LinkRule::rule(), LinkRule);
        self.register_rule(GenerateRule::rule(), GenerateRule);
        self.register_rule(TestRule::rule(), TestRule);
    }
}
