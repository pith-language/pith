use pith_core::{
    DeclarationTable, Interface,
    manifest::{encode_bytes, encode_length, encode_str},
};
use pith_hir::RuleCategory;
use pith_ids::ModuleAbiDigest;

pub const GRAMMAR_VERSION: u8 = 1;

pub struct RuleSignature {
    pub category: RuleCategory,
    pub interface: Interface,
}

pub fn abi_digest(
    module: &str,
    table: &DeclarationTable,
    imports: &[(Box<str>, ModuleAbiDigest)],
    signatures: &[RuleSignature],
) -> ModuleAbiDigest {
    let mut manifest = Vec::new();
    encode_str(&mut manifest, module);
    manifest.push(GRAMMAR_VERSION);
    manifest.push(pith_core::ENCODING_VERSION);

    encode_length(&mut manifest, table.len());
    for declaration in table.iter() {
        manifest.extend_from_slice(declaration.digest().digest().as_bytes());
    }

    encode_length(&mut manifest, imports.len());
    for (name, digest) in imports {
        encode_str(&mut manifest, name);
        manifest.extend_from_slice(digest.digest().as_bytes());
    }

    let mut provided: Vec<(u8, Vec<u8>)> = signatures
        .iter()
        .map(|signature| {
            (
                signature.category.abi_tag(),
                signature.interface.encode_canonical(),
            )
        })
        .collect();
    provided.sort();
    encode_length(&mut manifest, provided.len());
    for (category, interface) in provided {
        manifest.push(category);
        encode_bytes(&mut manifest, &interface);
    }

    ModuleAbiDigest::of_manifest(&manifest)
}
