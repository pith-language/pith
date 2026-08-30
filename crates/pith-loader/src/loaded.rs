//! What a module surface loads into: the declaration table, the rule
//! declarations, the ABI digest, and the sidecars tooling reads.

use std::collections::BTreeMap;
use std::sync::Arc;

use pith_core::{Action, DeclarationTable, Pure};
use pith_diag::{ByteOffset, Diag, SourceFile};
use pith_hir::{DefinitionKind, DefinitionLocation, PositionSidecar, SurfaceAbout};
use pith_ids::{ContentId, ModuleAbiDigest};

use crate::bind::{
    EntryDeclaration, HostRuleDeclaration, RepresentedRuleDeclaration, RuleDeclaration,
};
use crate::graph::InterfaceSurface;

pub struct LoadedModule {
    pub(crate) module: Box<str>,
    pub(crate) artifact_id: ContentId,
    pub(crate) source: Arc<SourceFile>,
    pub(crate) diagnostics: Box<[Diag]>,
    pub(crate) table: DeclarationTable,
    pub(crate) imports: Box<[(Box<str>, ModuleAbiDigest)]>,
    pub(crate) pure_rules: Box<[RuleDeclaration<Pure>]>,
    pub(crate) action_rules: Box<[RuleDeclaration<Action>]>,
    pub(crate) abi_digest: ModuleAbiDigest,
    pub(crate) positions: PositionSidecar,
    pub(crate) visible_imports: BTreeMap<Box<str>, Box<[DefinitionLocation]>>,
    pub(crate) entries: Box<[EntryDeclaration]>,
    pub(crate) about: Box<[SurfaceAbout]>,
}

impl LoadedModule {
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    #[must_use]
    pub const fn artifact_id(&self) -> ContentId {
        self.artifact_id
    }

    #[must_use]
    pub fn source(&self) -> &Arc<SourceFile> {
        &self.source
    }

    /// Diagnostics emitted while the module successfully elaborated.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }

    #[must_use]
    pub fn table(&self) -> &DeclarationTable {
        &self.table
    }

    #[must_use]
    pub fn imports(&self) -> &[(Box<str>, ModuleAbiDigest)] {
        &self.imports
    }

    #[must_use]
    pub fn pure_rules(&self) -> &[RuleDeclaration<Pure>] {
        &self.pure_rules
    }

    #[must_use]
    pub fn action_rules(&self) -> &[RuleDeclaration<Action>] {
        &self.action_rules
    }

    pub fn represented_pure_rules(
        &self,
    ) -> impl Iterator<Item = &RepresentedRuleDeclaration<Pure>> {
        self.pure_rules
            .iter()
            .filter_map(RuleDeclaration::as_represented)
    }

    #[must_use]
    pub fn pure_rule(&self, label: &str) -> Option<&HostRuleDeclaration<Pure>> {
        self.pure_rules
            .iter()
            .find(|rule| rule.coordinate().name.as_ref() == label)
            .and_then(RuleDeclaration::as_host)
    }

    #[must_use]
    pub fn represented_pure_rule(&self, label: &str) -> Option<&RepresentedRuleDeclaration<Pure>> {
        self.pure_rules
            .iter()
            .find(|rule| rule.coordinate().name.as_ref() == label)
            .and_then(RuleDeclaration::as_represented)
    }

    #[must_use]
    pub fn action_rule(&self, label: &str) -> Option<&HostRuleDeclaration<Action>> {
        self.action_rules
            .iter()
            .find(|rule| rule.coordinate().name.as_ref() == label)
            .and_then(RuleDeclaration::as_host)
    }

    #[must_use]
    pub fn represented_action_rule(
        &self,
        label: &str,
    ) -> Option<&RepresentedRuleDeclaration<Action>> {
        self.action_rules
            .iter()
            .find(|rule| rule.coordinate().name.as_ref() == label)
            .and_then(RuleDeclaration::as_represented)
    }

    #[must_use]
    pub const fn abi_digest(&self) -> ModuleAbiDigest {
        self.abi_digest
    }

    #[must_use]
    pub fn interface_surface(&self) -> InterfaceSurface {
        InterfaceSurface::of_module(self)
    }

    #[must_use]
    pub fn positions(&self) -> &PositionSidecar {
        &self.positions
    }

    /// Documentation blocks retained outside semantic digests.
    #[must_use]
    pub fn about(&self) -> &[SurfaceAbout] {
        &self.about
    }

    #[must_use]
    pub fn entries(&self) -> &[EntryDeclaration] {
        &self.entries
    }

    #[must_use]
    pub fn entry(&self, name: &str) -> Option<&EntryDeclaration> {
        self.entries.iter().find(|entry| entry.name() == name)
    }

    #[must_use]
    pub fn go_to_definition(&self, offset: ByteOffset) -> Option<&DefinitionLocation> {
        self.positions.definition_at(offset)
    }

    #[must_use]
    pub fn completions(&self, module: Option<&str>) -> Vec<&DefinitionLocation> {
        match module {
            None => self.positions.definitions().iter().collect(),
            Some(module) if module == self.module.as_ref() => {
                self.positions.definitions().iter().collect()
            }
            Some(module) => self
                .visible_imports
                .get(module)
                .map_or_else(Vec::new, |definitions| definitions.iter().collect()),
        }
    }
}

pub(crate) fn declaration_definitions(
    positions: &PositionSidecar,
) -> BTreeMap<Box<str>, DefinitionLocation> {
    positions
        .definitions()
        .iter()
        .filter(|definition| {
            matches!(
                definition.kind(),
                DefinitionKind::Nominal | DefinitionKind::Sum | DefinitionKind::Alias
            )
        })
        .map(|definition| (definition.coordinate().name.clone(), definition.clone()))
        .collect()
}
