//! Phloem's eager declaration table.

use std::sync::OnceLock;

use pith_core::{Coordinate, DeclarationDigest, DeclarationTable, Type};

pub const MODULE: &str = "phloem";

fn table() -> &'static DeclarationTable {
    static TABLE: OnceLock<DeclarationTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = DeclarationTable::new(MODULE);
        let source = crate::source::declare(&mut table);
        let origin = crate::lock::declare_origin(&mut table);
        let range = crate::constraint::declare_range(&mut table);
        let preference = crate::preference::declare(&mut table);
        crate::resolution::declare(&mut table, &source, &origin, &range, &preference);
        crate::identity::declare_version_scheme(&mut table);
        crate::substitution::model::declare(&mut table, &origin);
        table
    })
}

pub(crate) fn declared_name(spelling: &str) -> Box<str> {
    let coordinate = Coordinate::parse(spelling);
    assert!(
        coordinate.module.as_ref() == MODULE || coordinate.module.is_empty(),
        "phloem cannot declare `{spelling}`, which names module `{}`",
        coordinate.module
    );
    coordinate.name
}

pub(crate) fn declared_type(spelling: &str) -> Type {
    let name = &declared_name(spelling);
    match table().get(name) {
        Some(declaration) => Type::of_declaration(declaration),
        None => unreachable!("phloem's table declares `{name}` eagerly"),
    }
}

#[must_use]
pub fn spelling(name: &str) -> String {
    format!("{MODULE}.{name}")
}

#[must_use]
pub fn registered() -> Vec<(String, DeclarationDigest)> {
    table()
        .iter()
        .map(|declaration| (declaration.coordinate().spelling(), declaration.digest()))
        .collect()
}
