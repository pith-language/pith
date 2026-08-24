use pith_core::codec::{CanonicalDecodeError, CanonicalReader};
use pith_core::manifest::{encode_bytes, encode_length, encode_str};
use pith_core::{DeclarationTable, Interface};
use pith_elaborator::{GRAMMAR_VERSION, RuleSignature, abi_digest};
use pith_hir::RuleCategory;
use pith_ids::{ContentId, ModuleAbiDigest};

use crate::LoadedModule;

const INTERFACE_SURFACE_VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceSurface {
    pub(crate) module: Box<str>,
    imports: Box<[(Box<str>, ModuleAbiDigest)]>,
    pub(crate) table: DeclarationTable,
    provided: Box<[(RuleCategory, Interface)]>,
}

impl InterfaceSurface {
    pub(crate) fn of_parts(
        module: &str,
        imports: Box<[(Box<str>, ModuleAbiDigest)]>,
        table: DeclarationTable,
        provided: impl IntoIterator<Item = (RuleCategory, Interface)>,
    ) -> Self {
        let mut imports = imports.into_vec();
        imports.sort_by(|left, right| left.0.cmp(&right.0));
        let mut provided = provided.into_iter().collect::<Vec<_>>();
        provided.sort_by(|left, right| {
            (left.0, left.1.encode_canonical()).cmp(&(right.0, right.1.encode_canonical()))
        });
        Self {
            module: module.into(),
            imports: imports.into(),
            table,
            provided: provided.into(),
        }
    }

    #[must_use]
    pub fn of_module(loaded: &LoadedModule) -> Self {
        Self::of_parts(
            loaded.module(),
            loaded.imports().to_vec().into(),
            loaded.table().clone(),
            loaded
                .pure_rules()
                .iter()
                .map(|rule| (RuleCategory::Pure, rule.interface().clone()))
                .chain(
                    loaded
                        .action_rules()
                        .iter()
                        .map(|rule| (RuleCategory::Action, rule.interface().clone())),
                ),
        )
    }

    #[must_use]
    pub fn content_id(&self) -> ContentId {
        ContentId::of_blob(&self.encode())
    }

    #[must_use]
    pub fn abi_digest(&self) -> ModuleAbiDigest {
        let signatures = self
            .provided
            .iter()
            .map(|(category, interface)| RuleSignature {
                category: *category,
                interface: interface.clone(),
            })
            .collect::<Vec<_>>();
        abi_digest(&self.module, &self.table, &self.imports, &signatures)
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut artifact = vec![
            INTERFACE_SURFACE_VERSION,
            GRAMMAR_VERSION,
            pith_core::ENCODING_VERSION,
        ];
        encode_str(&mut artifact, &self.module);
        encode_bytes(&mut artifact, &self.table.encode_canonical());
        encode_length(&mut artifact, self.imports.len());
        for (name, digest) in &self.imports {
            encode_str(&mut artifact, name);
            artifact.extend_from_slice(digest.digest().as_bytes());
        }
        encode_length(&mut artifact, self.provided.len());
        for (category, interface) in &self.provided {
            artifact.push(category.abi_tag());
            encode_bytes(&mut artifact, &interface.encode_canonical());
        }
        artifact
    }

    /// # Errors
    /// Returns a decode error for a foreign version, truncated or trailing
    /// bytes, or a payload that does not decode.
    pub fn decode(encoded: &[u8]) -> Result<Self, CanonicalDecodeError> {
        let mut reader = CanonicalReader::new(encoded);
        reader.read_version(INTERFACE_SURFACE_VERSION)?;
        let grammar = reader.read_byte()?;
        let kernel = reader.read_byte()?;
        if grammar != GRAMMAR_VERSION {
            return Err(CanonicalDecodeError::UnsupportedVersion { version: grammar });
        }
        if kernel != pith_core::ENCODING_VERSION {
            return Err(CanonicalDecodeError::UnsupportedVersion { version: kernel });
        }
        let module: Box<str> = reader.read_text()?.into();
        let table = DeclarationTable::decode_canonical(reader.read_bytes()?)?;
        if table.module() != module.as_ref() {
            return Err(CanonicalDecodeError::DeclarationModuleMismatch {
                table: module,
                declaration: table.module().into(),
            });
        }
        let imports = reader
            .read_sequence(|reader| {
                let name: Box<str> = reader.read_text()?.into();
                let digest = ModuleAbiDigest::from_digest(reader.read_digest()?);
                Ok((name, digest))
            })?
            .to_vec();
        for [earlier, later] in imports.array_windows() {
            if earlier.0 >= later.0 {
                return Err(CanonicalDecodeError::NamesOutOfOrder {
                    earlier: earlier.0.clone(),
                    later: later.0.clone(),
                });
            }
        }
        let provided = reader
            .read_sequence(|reader| {
                let category = match reader.read_byte()? {
                    0 => RuleCategory::Pure,
                    1 => RuleCategory::Action,
                    tag => return Err(CanonicalDecodeError::UnknownValueTag { tag }),
                };
                let interface = Interface::decode_canonical(reader.read_bytes()?)?;
                Ok((category, interface))
            })?
            .to_vec();
        for [earlier, later] in provided.array_windows() {
            if (earlier.0, earlier.1.encode_canonical()) >= (later.0, later.1.encode_canonical()) {
                return Err(CanonicalDecodeError::NonCanonicalOrder {
                    sequence: "provided interfaces",
                });
            }
        }
        reader.finish()?;
        Ok(Self {
            module,
            imports: imports.into(),
            table,
            provided: provided.into(),
        })
    }
}
