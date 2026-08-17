//! Phloem's declaration table (decision 0047).
//!
//! Every nominal type and declared sum phloem owns is registered here once, on
//! first use, under the module identity its rule identities already carry. The
//! construction sites elsewhere in the crate ask for a declared type by name
//! rather than building `Type::Nominal { name }` or an inline constructor set,
//! so a sum's constructor list lives in one place and a rule revision can derive
//! from the declarations its interface names.
//!
//! Registration is lazy and one-shot because the table is a property of the
//! crate rather than of a run: the declarations do not depend on a lock, a
//! registry answer, or anything a caller supplies.

use std::sync::{Mutex, OnceLock};

use pith_core::{Coordinate, DeclarationTable, SumConstructor, Type};

/// The module identity phloem's declarations are registered under.
pub const MODULE: &str = "phloem";

/// The table, behind a mutex because declarations are registered on first use
/// from whichever call site reaches one first. Contention is limited to the
/// first touch of each declaration.
fn table() -> &'static Mutex<DeclarationTable> {
    static TABLE: OnceLock<Mutex<DeclarationTable>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(DeclarationTable::new(MODULE)))
}

fn with_table<T>(body: impl FnOnce(&mut DeclarationTable) -> T) -> T {
    match table().lock() {
        Ok(mut guard) => body(&mut guard),
        // A poisoned lock means a declaration panicked while registering, which
        // the two constructors below cannot do: they refuse only a duplicate
        // name, and a duplicate is served from the table instead.
        Err(poisoned) => body(&mut poisoned.into_inner()),
    }
}

/// The declared name inside a coordinate spelling.
///
/// The crate's `pub const` names are the spellings values carry, and they are
/// the single place each declared name is written; the table keys on the short
/// name, so it is derived here rather than restated. A spelling from another
/// module would be a programming error at a declaration site, and the module
/// half is checked rather than assumed.
fn declared_name(spelling: &str) -> Box<str> {
    let coordinate = Coordinate::parse(spelling);
    assert!(
        coordinate.module.as_ref() == MODULE || coordinate.module.is_empty(),
        "phloem cannot declare `{spelling}`, which names module `{}`",
        coordinate.module
    );
    coordinate.name
}

/// The declared nominal type named by the coordinate spelling `spelling`, over
/// `representation`, registering it on first use and returning the same
/// declaration afterwards.
pub(crate) fn nominal(spelling: &str, representation: Type) -> Type {
    let name = &declared_name(spelling);
    with_table(|table| {
        if let Some(declared) = table.get(name) {
            return Type::of_declaration(declared);
        }
        match table.nominal(name, representation) {
            Ok(declared) => declared,
            Err(error) => unreachable!("the name was absent a statement ago: {error}"),
        }
    })
}

/// The declared sum named by the coordinate spelling `spelling`, on the same
/// terms as [`nominal`].
pub(crate) fn sum<const N: usize>(spelling: &str, constructors: [SumConstructor; N]) -> Type {
    let name = &declared_name(spelling);
    with_table(|table| {
        if let Some(declared) = table.get(name) {
            return Type::of_declaration(declared);
        }
        match table.sum(name, constructors) {
            Ok(declared) => declared,
            Err(error) => unreachable!("the name was absent a statement ago: {error}"),
        }
    })
}

/// The coordinate spelling a value of `phloem.<name>` carries.
#[must_use]
pub fn spelling(name: &str) -> String {
    format!("{MODULE}.{name}")
}

/// Every declaration phloem has registered so far, as `(spelling, digest)`
/// pairs in name order.
///
/// Lazy registration means this is the set the crate has *reached*, not the set
/// it will eventually hold, so a revision derived from it must name the
/// declarations its own interface uses rather than iterating this.
#[must_use]
pub fn registered() -> Vec<(String, pith_core::DeclarationDigest)> {
    with_table(|table| {
        table
            .iter()
            .map(|declaration| (declaration.coordinate().spelling(), declaration.digest()))
            .collect()
    })
}
