//! The import environment: which modules a module's elaboration may name.

pub use pith_elaborator::ImportedModule;

use pith_core::{DeclarationError, DeclarationTable, RecordField, Type};

use crate::LoadedModule;
use crate::graph::InterfaceSurface;
use pith_hir::DefinitionLocation;

pub const BUILTIN_MODULE: &str = "pith";

/// The caller effect accepted by `pith exec`: a program path and its ordered
/// argument vector. It is nominal because executing a structurally similar
/// record accidentally would cross an effect boundary.
///
/// # Errors
/// Returns a declaration error if the builtin table no longer declares one
/// valid `Exec` nominal.
pub fn exec_type() -> Result<Type, DeclarationError> {
    builtin_table().map(|(_, exec)| exec)
}

fn exec_representation() -> Type {
    Type::Record(Box::new([
        RecordField {
            name: "arguments".into(),
            payload: Type::List(Box::new(Type::Text)),
        },
        RecordField {
            name: "program".into(),
            payload: Type::Text,
        },
    ]))
}

fn builtin_table() -> Result<(DeclarationTable, Type), DeclarationError> {
    let mut table = DeclarationTable::new(BUILTIN_MODULE);
    let exec = table.nominal("Exec", exec_representation())?;
    Ok((table, exec))
}

#[derive(Default)]
pub struct ImportEnv {
    pub(crate) inner: pith_elaborator::ImportEnv,
}

impl ImportEnv {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct the import environment visible to source drivers. Builtins
    /// still require an explicit `import pith`; this only makes that import
    /// resolvable without inventing a file-relative module.
    ///
    /// # Errors
    /// Returns a declaration error if the builtin nominal table is invalid.
    pub fn with_builtins() -> Result<Self, DeclarationError> {
        let (table, _) = builtin_table()?;
        let abi = pith_elaborator::abi_digest(BUILTIN_MODULE, &table, &[], &[]);
        let mut environment = Self::new();
        environment.inner.insert(
            BUILTIN_MODULE,
            abi,
            table,
            Box::<[DefinitionLocation]>::default(),
        );
        Ok(environment)
    }

    pub fn insert_loaded(&mut self, loaded: &LoadedModule) {
        self.inner.insert(
            loaded.module.clone(),
            loaded.abi_digest,
            loaded.table.clone(),
            loaded.positions.definitions(),
        );
    }

    pub(crate) fn insert_surface(&mut self, binding: &str, surface: &InterfaceSurface) {
        self.inner.insert(
            binding,
            surface.abi_digest(),
            surface.table.clone(),
            Box::<[DefinitionLocation]>::default(),
        );
    }

    #[must_use]
    pub fn get(&self, module: &str) -> Option<&ImportedModule> {
        self.inner.get(module)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
