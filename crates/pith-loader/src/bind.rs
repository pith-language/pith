//! The typed rule declarations a module surface produces, and the binding
//! lifecycle that carries them onto an engine: host bodies bind through the
//! kernel's registration calls, represented bodies register as data.

use std::marker::PhantomData;

use pith_core::{
    Action, BodyError, BodyRevision, Coordinate, Interface, Pure, Request, Rule, RuleBody, RuleId,
};
use pith_elaborator::Visibility;
use pith_engine::{ActionRule, Engine, EngineStateReader, PureRule};

pub enum RuleDeclaration<K> {
    Host(HostRuleDeclaration<K>),
    Represented(RepresentedRuleDeclaration<K>),
}

impl<K> RuleDeclaration<K> {
    fn metadata(&self) -> &DeclarationMetadata<K> {
        match self {
            Self::Host(declaration) => &declaration.metadata,
            Self::Represented(declaration) => &declaration.metadata,
        }
    }

    #[must_use]
    pub fn coordinate(&self) -> &Coordinate {
        &self.metadata().coordinate
    }

    #[must_use]
    pub fn interface(&self) -> &Interface {
        &self.metadata().interface
    }

    #[must_use]
    pub fn span(&self) -> pith_diag::Span {
        self.metadata().span
    }

    /// Whether the rule is exported to importers or module-private.
    #[must_use]
    pub const fn visibility(&self) -> Visibility {
        match self {
            Self::Host(declaration) => declaration.metadata.visibility,
            Self::Represented(declaration) => declaration.metadata.visibility,
        }
    }

    #[must_use]
    pub const fn is_represented(&self) -> bool {
        matches!(self, Self::Represented(_))
    }

    #[must_use]
    pub const fn as_host(&self) -> Option<&HostRuleDeclaration<K>> {
        match self {
            Self::Host(declaration) => Some(declaration),
            Self::Represented(_) => None,
        }
    }

    #[must_use]
    pub const fn as_represented(&self) -> Option<&RepresentedRuleDeclaration<K>> {
        match self {
            Self::Host(_) => None,
            Self::Represented(declaration) => Some(declaration),
        }
    }

    #[must_use]
    pub fn represented_digest(&self) -> Option<pith_ids::BodyIrDigest> {
        self.as_represented()
            .map(RepresentedRuleDeclaration::digest)
    }
}

pub(crate) struct DeclarationMetadata<K> {
    coordinate: Coordinate,
    interface: Interface,
    span: pith_diag::Span,
    visibility: Visibility,
    effect: PhantomData<fn() -> K>,
}

impl<K> DeclarationMetadata<K> {
    pub(crate) fn new(
        module: &str,
        label: Box<str>,
        interface: Interface,
        span: pith_diag::Span,
        visibility: Visibility,
    ) -> Self {
        Self {
            coordinate: Coordinate::new(module, label),
            interface,
            span,
            visibility,
            effect: PhantomData,
        }
    }
}

pub struct HostRuleDeclaration<K> {
    metadata: DeclarationMetadata<K>,
}

impl<K> HostRuleDeclaration<K> {
    pub(crate) fn new(metadata: DeclarationMetadata<K>) -> Self {
        Self { metadata }
    }

    #[must_use]
    pub fn coordinate(&self) -> &Coordinate {
        &self.metadata.coordinate
    }

    #[must_use]
    pub fn interface(&self) -> &Interface {
        &self.metadata.interface
    }

    #[must_use]
    pub const fn span(&self) -> pith_diag::Span {
        self.metadata.span
    }
}

impl HostRuleDeclaration<Pure> {
    #[must_use]
    pub fn rule(&self, body_revision: BodyRevision) -> Rule<Pure> {
        Rule::declared(
            &self.metadata.coordinate.module,
            &self.metadata.coordinate.name,
            body_revision,
            self.metadata.interface.clone(),
            self.metadata.span,
        )
    }

    pub fn bind<S, B>(&self, engine: &mut Engine<S>, body_revision: BodyRevision, body: B) -> RuleId
    where
        S: EngineStateReader + ?Sized,
        B: PureRule + 'static,
    {
        engine.register_rule(self.rule(body_revision), body)
    }
}

impl HostRuleDeclaration<Action> {
    #[must_use]
    pub fn rule(&self, body_revision: BodyRevision) -> Rule<Action> {
        Rule::declared(
            &self.metadata.coordinate.module,
            &self.metadata.coordinate.name,
            body_revision,
            self.metadata.interface.clone(),
            self.metadata.span,
        )
    }

    pub fn bind<S, B>(&self, engine: &mut Engine<S>, body_revision: BodyRevision, body: B) -> RuleId
    where
        S: EngineStateReader + ?Sized,
        B: ActionRule + 'static,
    {
        engine.register_action_rule(self.rule(body_revision), body)
    }
}

pub struct RepresentedRuleDeclaration<K> {
    metadata: DeclarationMetadata<K>,
    body: RuleBody,
}

pub struct EntryDeclaration {
    module: Box<str>,
    name: Box<str>,
    interface: Interface,
    span: pith_diag::Span,
    body: RuleBody,
}

impl EntryDeclaration {
    pub(crate) fn new(
        module: &str,
        name: Box<str>,
        interface: Interface,
        span: pith_diag::Span,
        body: RuleBody,
    ) -> Self {
        Self {
            module: module.into(),
            name,
            interface,
            span,
            body,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn interface(&self) -> &Interface {
        &self.interface
    }

    #[must_use]
    pub const fn span(&self) -> pith_diag::Span {
        self.span
    }

    #[must_use]
    pub fn coordinate(&self) -> Coordinate {
        Coordinate::new(self.module.as_ref(), self.rule_label())
    }

    /// The rule label the entry's body registers under: the entry's name in
    /// the module's `entry` namespace. The query surface reads registrations
    /// back through this spelling, so the convention has one definition.
    #[must_use]
    pub fn rule_label(&self) -> String {
        format!("entry.{}", self.name)
    }

    #[must_use]
    pub fn rule(&self) -> Rule<Pure> {
        Rule::represented(
            &self.module,
            &self.rule_label(),
            &self.body,
            self.interface.clone(),
            self.span,
        )
    }

    /// The request a run of this entry issues. Its label is prose for
    /// diagnostics, deliberately not the [`Self::rule_label`] spelling the
    /// body registers under.
    #[must_use]
    pub fn request(&self) -> Request<Pure> {
        Request::new(
            format!("entry {}", self.name),
            self.interface.clone(),
            [],
            self.span,
        )
    }

    /// # Errors
    /// Returns [`BodyError`] if the elaborated body no longer checks against
    /// the entry interface.
    pub fn register<S>(&self, engine: &mut Engine<S>) -> Result<RuleId, BodyError>
    where
        S: EngineStateReader + ?Sized,
    {
        engine.register_represented_rule(
            &self.module,
            &self.rule_label(),
            self.interface.clone(),
            self.span,
            self.body.clone(),
        )
    }
}

impl<K> RepresentedRuleDeclaration<K> {
    pub(crate) fn new(metadata: DeclarationMetadata<K>, body: RuleBody) -> Self {
        Self { metadata, body }
    }

    #[must_use]
    pub fn coordinate(&self) -> &Coordinate {
        &self.metadata.coordinate
    }

    #[must_use]
    pub fn interface(&self) -> &Interface {
        &self.metadata.interface
    }

    #[must_use]
    pub const fn span(&self) -> pith_diag::Span {
        self.metadata.span
    }

    #[must_use]
    pub fn digest(&self) -> pith_ids::BodyIrDigest {
        self.body.digest()
    }

    #[must_use]
    pub fn body(&self) -> &RuleBody {
        &self.body
    }
}

impl RepresentedRuleDeclaration<Pure> {
    /// # Errors
    /// Returns [`BodyError`] when the body does not check against the interface.
    pub fn register<S>(&self, engine: &mut Engine<S>) -> Result<RuleId, BodyError>
    where
        S: EngineStateReader + ?Sized,
    {
        engine.register_represented_rule(
            &self.metadata.coordinate.module,
            &self.metadata.coordinate.name,
            self.metadata.interface.clone(),
            self.metadata.span,
            self.body.clone(),
        )
    }
}
