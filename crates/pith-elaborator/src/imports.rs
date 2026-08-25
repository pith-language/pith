use std::collections::BTreeMap;

use pith_core::DeclarationTable;
use pith_diag::Diag;
use pith_hir::{DefinitionKind, DefinitionLocation, FrontendCode, ModuleFiles, ParsedSurface};
use pith_ids::ModuleAbiDigest;

#[derive(Clone)]
pub struct ImportedModule {
    pub(crate) abi_digest: ModuleAbiDigest,
    pub(crate) table: DeclarationTable,
    definitions: Box<[DefinitionLocation]>,
}

impl ImportedModule {
    #[must_use]
    pub const fn abi_digest(&self) -> ModuleAbiDigest {
        self.abi_digest
    }

    #[must_use]
    pub fn definitions(&self) -> &[DefinitionLocation] {
        &self.definitions
    }

    pub(crate) fn declaration_definition(&self, name: &str) -> Option<&DefinitionLocation> {
        self.definitions.iter().find(|definition| {
            definition.coordinate().name.as_ref() == name
                && !matches!(definition.kind(), DefinitionKind::HostRule(_))
        })
    }
}

#[derive(Default)]
pub struct ImportEnv {
    modules: BTreeMap<Box<str>, ImportedModule>,
}

impl ImportEnv {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        binding: impl Into<Box<str>>,
        abi_digest: ModuleAbiDigest,
        table: DeclarationTable,
        definitions: impl Into<Box<[DefinitionLocation]>>,
    ) {
        self.modules.insert(
            binding.into(),
            ImportedModule {
                abi_digest,
                table,
                definitions: definitions.into(),
            },
        );
    }

    #[must_use]
    pub fn get(&self, module: &str) -> Option<&ImportedModule> {
        self.modules.get(module)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

pub struct ScopedImports<'a> {
    modules: BTreeMap<&'a str, &'a ImportedModule>,
}

impl<'a> ScopedImports<'a> {
    #[must_use]
    pub fn get(&self, module: &str) -> Option<&'a ImportedModule> {
        self.modules.get(module).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &ImportedModule)> {
        self.modules.iter().map(|(name, module)| (*name, *module))
    }
}

pub fn scope_imports<'a>(
    surface: &'a ParsedSurface,
    environment: &'a ImportEnv,
    files: &ModuleFiles,
    diagnostics: &mut Vec<Diag>,
) -> ScopedImports<'a> {
    let mut modules = BTreeMap::new();
    for import in &surface.imports {
        if modules.contains_key(import.module.as_ref()) {
            diagnostics.push(files.error(
                FrontendCode::DuplicateImport,
                import.span,
                format!("module `{}` is imported twice", import.module),
            ));
            continue;
        }
        let Some(imported) = environment.get(&import.module) else {
            diagnostics.push(files.error(
                FrontendCode::UnknownImport,
                import.span,
                format!("module `{}` is not available to import", import.module),
            ));
            continue;
        };
        modules.insert(import.module.as_ref(), imported);
    }
    ScopedImports { modules }
}
