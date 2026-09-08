//! What a module surface loads into: the declaration table, the rule
//! declarations, the ABI digest, and the sidecars tooling reads.

use std::collections::BTreeMap;

use pith_core::{Action, DeclarationTable, Pure};
use pith_diag::{ByteOffset, Diag, SourceFile};
use pith_hir::{DefinitionKind, DefinitionLocation, ModuleFiles, PositionSidecar, SurfaceAbout};
use pith_ids::{ContentId, ModuleAbiDigest};

use crate::bind::{
    EntryDeclaration, HostRuleDeclaration, RepresentedRuleDeclaration, RuleDeclaration,
};
use crate::graph::InterfaceSurface;

pub struct LoadedModule {
    pub(crate) module: Box<str>,
    pub(crate) artifact_id: ContentId,
    pub(crate) files: ModuleFiles,
    pub(crate) diagnostics: Box<[Diag]>,
    pub(crate) table: DeclarationTable,
    pub(crate) imports: pith_elaborator::ImportedAbis,
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

    /// The module's files, mapping every elaborated span back to the file
    /// that owns it: one file in standalone mode, a sorted set in manifest
    /// mode.
    #[must_use]
    pub const fn files(&self) -> &ModuleFiles {
        &self.files
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

    /// The imported subject/ABI pairs in canonical form.
    #[must_use]
    pub fn imports(&self) -> &[(Box<str>, ModuleAbiDigest)] {
        self.imports.as_slice()
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

    /// The definition visible at `offset` within `source`. Offsets are
    /// file-local; a module of several files answers from the file named.
    #[must_use]
    pub fn go_to_definition(
        &self,
        source: &SourceFile,
        offset: ByteOffset,
    ) -> Option<&DefinitionLocation> {
        self.positions.definition_at(source, offset)
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
