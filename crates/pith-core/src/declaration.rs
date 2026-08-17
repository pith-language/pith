//! Declarations and the per-module table that holds them (decision 0047).
//!
//! Three things are declarable: a nominal type over a structural
//! representation, a declared sum over a fixed constructor set, and a structural
//! alias that expands to its target. A declaration's identity is its
//! *coordinate* — the module identity plus the declared name — and its revision
//! is a digest over its body. That is decision 0023's two-halves shape applied
//! to types: the coordinate survives a representation change, the digest does
//! not.
//!
//! The table is what makes a coordinate meaningful. It refuses a module that
//! declares one name twice, and it refuses a recursive alias, which has no
//! finite canonical form because expansion is its only semantics.

use pith_ids::DeclarationDigest;

use crate::manifest::{encode_length, encode_str};
use crate::value::{SumConstructor, Type};
use crate::value_codec::encode_type_payload as encode_type_manifest;

/// The stable coordinate of a declaration: a module identity and a declared
/// name (decision 0047).
///
/// Two modules declaring the same short name are two declarations, because the
/// key is the pair. The module identity is accepted at the registration
/// boundary, as decision 0023 already accepts for rule identities; what a
/// module identity is beyond a string stays 0023's and 0038's open question.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Coordinate {
    pub module: Box<str>,
    pub name: Box<str>,
}

impl Coordinate {
    pub fn new(module: impl Into<Box<str>>, name: impl Into<Box<str>>) -> Self {
        Self {
            module: module.into(),
            name: name.into(),
        }
    }

    /// Split a dotted spelling back into a coordinate, for the best-effort
    /// declaration [`crate::Value::value_type`] synthesizes from a value's name.
    ///
    /// The last dot separates the module from the name, so a module identity
    /// containing a dot round-trips as a longer module and the shorter name.
    /// That ambiguity is recorded in decision 0047; the prototype's population
    /// is single-segment module identities.
    #[must_use]
    pub fn parse(spelling: &str) -> Self {
        match spelling.rsplit_once('.') {
            Some((module, name)) => Self::new(module, name),
            None => Self::new("", spelling),
        }
    }

    /// The dotted spelling a value carries and a diagnostic renders.
    ///
    /// A value names its type with this string rather than with a coordinate,
    /// because a value is data (decision 0047). The spelling is ambiguous when a
    /// module identity contains a dot; the prototype's population is
    /// single-segment module identities, and the grammar belongs to the
    /// module-system record.
    /// Inverse of [`Self::parse`] on every input, including a dotless one: a
    /// coordinate with no module spells as the bare name, so the best-effort
    /// declaration `value_type` synthesizes from a dotless value name spells
    /// back to that name and reflexivity holds.
    #[must_use]
    pub fn spelling(&self) -> String {
        if self.module.is_empty() {
            return self.name.as_ref().to_owned();
        }
        format!("{}.{}", self.module, self.name)
    }
}

/// What a declaration declares.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DeclarationBody {
    /// A nominal type over a structural representation. `xylem.CSource` over
    /// `Blob` is one. Two nominals over the same representation are distinct,
    /// which is the reason identity is a coordinate rather than a content hash.
    Nominal { representation: Type },
    /// A declared sum over a fixed constructor set, sorted by constructor name.
    Sum { constructors: Box<[SumConstructor]> },
    /// A structural abbreviation. Referencing an alias yields its target,
    /// expanded, so an alias has no coordinate at a use site and cannot be
    /// recursive.
    Alias { target: Type },
}

impl DeclarationBody {
    /// The tag this body's kind contributes to a declaration's digest. Kept
    /// beside the variants so a new kind cannot be added without giving it one.
    const fn kind_tag(&self) -> u8 {
        match self {
            Self::Nominal { .. } => 0,
            Self::Sum { .. } => 1,
            Self::Alias { .. } => 2,
        }
    }
}

/// One entry in a module's declaration table.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Declaration {
    coordinate: Coordinate,
    body: DeclarationBody,
}

impl Declaration {
    pub fn coordinate(&self) -> &Coordinate {
        &self.coordinate
    }

    pub fn body(&self) -> &DeclarationBody {
        &self.body
    }

    /// The declaration's revision: a domain-separated digest over its
    /// coordinate, its kind, and the canonical encoding of its body.
    ///
    /// Three things do not participate, on the ground that a digest must not
    /// move for what no reader can observe: a doc comment, the declaration's
    /// position in its table, and formatting. One thing does: a change to what
    /// the declaration says.
    #[must_use]
    pub fn digest(&self) -> DeclarationDigest {
        let mut manifest = Vec::new();
        encode_str(&mut manifest, &self.coordinate.module);
        encode_str(&mut manifest, &self.coordinate.name);
        manifest.push(self.body.kind_tag());
        match &self.body {
            DeclarationBody::Nominal { representation } => {
                encode_type_manifest(&mut manifest, representation);
            }
            DeclarationBody::Sum { constructors } => {
                encode_length(&mut manifest, constructors.len());
                for constructor in constructors {
                    encode_str(&mut manifest, &constructor.name);
                    match &constructor.payload {
                        Some(payload) => {
                            manifest.push(1);
                            encode_type_manifest(&mut manifest, payload);
                        }
                        None => manifest.push(0),
                    }
                }
            }
            DeclarationBody::Alias { target } => encode_type_manifest(&mut manifest, target),
        }
        DeclarationDigest::of_manifest(&manifest)
    }
}

/// Why a declaration could not be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclarationError {
    /// The module already declares this name. The table's key is the pair, so
    /// this is a collision within one module and never across two.
    DuplicateName { module: Box<str>, name: Box<str> },
    /// An alias whose target reaches itself. Referencing an alias yields its
    /// target expanded, so a recursive one has no finite canonical form:
    /// expansion is its only semantics and it does not terminate. A nominal or
    /// a sum may recurse, because its occurrence inside its own body is a cut
    /// rather than an expansion.
    RecursiveAlias { module: Box<str>, name: Box<str> },
}

impl std::fmt::Display for DeclarationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateName { module, name } => {
                write!(f, "module `{module}` already declares `{name}`")
            }
            Self::RecursiveAlias { module, name } => write!(
                f,
                "alias `{module}.{name}` refers to itself, and an alias has no spelling to \
                 recurse through: declare it as a nominal type or a sum instead"
            ),
        }
    }
}

impl std::error::Error for DeclarationError {}

/// A module's declarations, keyed by declared name.
///
/// Registration order is not content: two tables holding the same declarations
/// in different orders derive the same digest per declaration, which is what
/// keeps a reordering from moving a rule's revision. Entries are held in sorted
/// order so iteration is deterministic without an ordered map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarationTable {
    module: Box<str>,
    entries: Vec<Declaration>,
}

impl DeclarationTable {
    pub fn new(module: impl Into<Box<str>>) -> Self {
        Self {
            module: module.into(),
            entries: Vec::new(),
        }
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    /// Declare a nominal type over `representation`.
    ///
    /// # Errors
    /// [`DeclarationError::DuplicateName`] when the module already declares the
    /// name.
    pub fn nominal(&mut self, name: &str, representation: Type) -> Result<Type, DeclarationError> {
        self.declare(name, DeclarationBody::Nominal { representation })
    }

    /// Declare a sum over `constructors`, sorted by constructor name.
    ///
    /// # Errors
    /// [`DeclarationError::DuplicateName`] when the module already declares the
    /// name, and [`crate::DuplicateNameError`] surfaces as a duplicate
    /// constructor through [`Type::sum`], so constructors are sorted here and
    /// a repeated constructor name is refused.
    pub fn sum(
        &mut self,
        name: &str,
        constructors: impl Into<Box<[SumConstructor]>>,
    ) -> Result<Type, DeclarationError> {
        let constructors = crate::value::sorted_constructors(constructors).map_err(|error| {
            DeclarationError::DuplicateName {
                module: self.module.clone(),
                name: error.name,
            }
        })?;
        self.declare(name, DeclarationBody::Sum { constructors })
    }

    /// Declare a structural alias for `target`.
    ///
    /// # Errors
    /// [`DeclarationError::DuplicateName`] when the module already declares the
    /// name, and [`DeclarationError::RecursiveAlias`] when `target` reaches the
    /// alias being declared.
    pub fn alias(&mut self, name: &str, target: Type) -> Result<Type, DeclarationError> {
        if target.reaches_cut() {
            return Err(DeclarationError::RecursiveAlias {
                module: self.module.clone(),
                name: name.into(),
            });
        }
        self.declare(name, DeclarationBody::Alias { target })
    }

    fn declare(&mut self, name: &str, body: DeclarationBody) -> Result<Type, DeclarationError> {
        let coordinate = Coordinate::new(self.module.clone(), name);
        match self
            .entries
            .binary_search_by(|entry| entry.coordinate.name.as_ref().cmp(name))
        {
            Ok(_) => {
                return Err(DeclarationError::DuplicateName {
                    module: self.module.clone(),
                    name: name.into(),
                });
            }
            Err(position) => self.entries.insert(
                position,
                Declaration {
                    coordinate: coordinate.clone(),
                    body: body.clone(),
                },
            ),
        }
        Ok(Type::of_declaration(&Declaration { coordinate, body }))
    }

    /// The declaration this module holds under `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Declaration> {
        self.entries
            .binary_search_by(|entry| entry.coordinate.name.as_ref().cmp(name))
            .ok()
            .and_then(|position| self.entries.get(position))
    }

    /// Every declaration in this module, in name order.
    pub fn iter(&self) -> impl Iterator<Item = &Declaration> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecordField;

    fn table() -> DeclarationTable {
        DeclarationTable::new("test")
    }

    #[test]
    fn a_module_declaring_one_name_twice_is_refused() {
        let mut table = table();
        assert!(table.nominal("CSource", Type::Blob).is_ok());
        assert_eq!(
            table.nominal("CSource", Type::Text),
            Err(DeclarationError::DuplicateName {
                module: "test".into(),
                name: "CSource".into(),
            })
        );
    }

    #[test]
    fn two_modules_declaring_one_short_name_are_two_declarations() {
        // The table's key is the pair, so this is not a collision (0047).
        let mut xylem = DeclarationTable::new("xylem");
        let mut phloem = DeclarationTable::new("phloem");
        let one = xylem.nominal("Source", Type::Blob).unwrap();
        let other = phloem.nominal("Source", Type::Blob).unwrap();
        assert_ne!(one, other);
        assert_ne!(
            xylem.get("Source").unwrap().digest(),
            phloem.get("Source").unwrap().digest()
        );
    }

    #[test]
    fn registration_order_does_not_move_a_digest() {
        // The claim the revision derivation rests on: reordering a table is not
        // a change to any declaration in it (0047).
        let mut ascending = table();
        ascending.nominal("A", Type::Blob).unwrap();
        ascending.nominal("B", Type::Text).unwrap();
        let mut descending = table();
        descending.nominal("B", Type::Text).unwrap();
        descending.nominal("A", Type::Blob).unwrap();

        for name in ["A", "B"] {
            assert_eq!(
                ascending.get(name).unwrap().digest(),
                descending.get(name).unwrap().digest()
            );
        }
    }

    #[test]
    fn a_changed_representation_moves_the_digest_and_a_changed_module_does_too() {
        let mut over_blob = table();
        let mut over_text = table();
        over_blob.nominal("CSource", Type::Blob).unwrap();
        over_text.nominal("CSource", Type::Text).unwrap();
        assert_ne!(
            over_blob.get("CSource").unwrap().digest(),
            over_text.get("CSource").unwrap().digest()
        );

        let mut elsewhere = DeclarationTable::new("other");
        elsewhere.nominal("CSource", Type::Blob).unwrap();
        assert_ne!(
            over_blob.get("CSource").unwrap().digest(),
            elsewhere.get("CSource").unwrap().digest()
        );
    }

    #[test]
    fn a_changed_constructor_set_moves_a_sums_digest() {
        let one = |payload: Option<Type>| {
            let mut table = table();
            table
                .sum(
                    "Source",
                    [SumConstructor {
                        name: "Archive".into(),
                        payload,
                    }],
                )
                .unwrap();
            table.get("Source").unwrap().digest()
        };
        assert_ne!(one(None), one(Some(Type::Blob)));
        assert_ne!(one(Some(Type::Blob)), one(Some(Type::Text)));
        assert_eq!(one(Some(Type::Blob)), one(Some(Type::Blob)));
    }

    #[test]
    fn an_alias_yields_its_target_expanded() {
        let mut table = table();
        let alias = table
            .alias("Headers", Type::List(Box::new(Type::Blob)))
            .unwrap();
        // An alias has no spelling at a use site, so referencing it is
        // indistinguishable from writing the target (0047).
        assert_eq!(alias, Type::List(Box::new(Type::Blob)));
    }

    #[test]
    fn a_recursive_alias_is_refused_and_a_recursive_nominal_is_not() {
        let mut table = table();
        assert_eq!(
            table.alias("Loop", Type::List(Box::new(Type::Cut))),
            Err(DeclarationError::RecursiveAlias {
                module: "test".into(),
                name: "Loop".into(),
            })
        );
        // The same shape as a nominal is fine: the cut names the declaration
        // being declared, so the canonical form is finite.
        assert!(
            table
                .nominal("Tree", Type::List(Box::new(Type::Cut)))
                .is_ok()
        );
    }

    #[test]
    fn a_cut_reached_only_through_another_declaration_does_not_make_an_alias_recursive() {
        let mut table = table();
        let tree = table
            .nominal("Tree", Type::List(Box::new(Type::Cut)))
            .unwrap();
        // The cut inside `Tree` belongs to `Tree` and is finite there, so an
        // alias for a record holding one expands fine.
        assert!(
            table
                .alias(
                    "Forest",
                    Type::record([RecordField {
                        name: "root".into(),
                        payload: tree,
                    }])
                    .unwrap(),
                )
                .is_ok()
        );
    }
}
