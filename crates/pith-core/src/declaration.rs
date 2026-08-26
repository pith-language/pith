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
use pith_output::dto::{DeclarationBodyRepr, DeclarationView, SumConstructorRepr};

use crate::codec::{CanonicalDecodeError, CanonicalReader};
use crate::manifest::{encode_bytes, encode_length, encode_str};
use crate::value::{SumConstructor, Type};
use crate::value_codec::{decode_type_payload, encode_type_payload as encode_type_manifest};

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
    ///
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
        DeclarationDigest::of_manifest(&self.encode_canonical())
    }

    /// Encode the coordinate, kind, and body canonically.
    #[must_use]
    pub fn encode_canonical(&self) -> Vec<u8> {
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
        manifest
    }

    /// Decode one declaration from its canonical encoding.
    ///
    /// # Errors
    /// Returns the reader's error for truncated input, an unknown body kind,
    /// or a representation that does not decode.
    pub fn decode_canonical(encoded: &[u8]) -> Result<Self, CanonicalDecodeError> {
        let mut decoder = CanonicalReader::new(encoded);
        let module: Box<str> = decoder.read_text()?.into();
        let name: Box<str> = decoder.read_text()?.into();
        if name.is_empty() || name.contains('.') {
            return Err(CanonicalDecodeError::InvalidDeclarationName { name });
        }
        let body = match decoder.read_byte()? {
            0 => DeclarationBody::Nominal {
                representation: decode_type_payload(&mut decoder)?,
            },
            1 => {
                let constructors = decoder.read_sequence(|decoder| {
                    let name: Box<str> = decoder.read_text()?.into();
                    let payload = match decoder.read_bool()? {
                        true => Some(decode_type_payload(decoder)?),
                        false => None,
                    };
                    Ok(SumConstructor { name, payload })
                })?;
                for [earlier, later] in constructors.array_windows() {
                    if earlier.name >= later.name {
                        return Err(CanonicalDecodeError::NamesOutOfOrder {
                            earlier: earlier.name.clone(),
                            later: later.name.clone(),
                        });
                    }
                }
                DeclarationBody::Sum { constructors }
            }
            2 => {
                let target = decode_type_payload(&mut decoder)?;
                if target.reaches_cut() {
                    return Err(CanonicalDecodeError::RecursiveAlias { module, name });
                }
                DeclarationBody::Alias { target }
            }
            kind => {
                return Err(CanonicalDecodeError::UnknownDeclarationKind { kind });
            }
        };
        decoder.finish()?;
        Ok(Self {
            coordinate: Coordinate { module, name },
            body,
        })
    }
}

/// The declaration grammar's spelling of a body. This is the source form a
/// person wrote, so tooling that shows a declaration back — `pith explore`,
/// a hover — echoes the grammar instead of inventing a second notation for
/// one shape.
impl std::fmt::Display for DeclarationBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nominal { representation } => write!(f, "{representation}"),
            Self::Alias { target } => write!(f, "{target}"),
            Self::Sum { constructors } => {
                let mut separator = "";
                for constructor in constructors {
                    f.write_str(separator)?;
                    match &constructor.payload {
                        Some(payload) => write!(f, "{}({payload})", constructor.name)?,
                        None => f.write_str(&constructor.name)?,
                    }
                    separator = " | ";
                }
                Ok(())
            }
        }
    }
}

impl From<&DeclarationBody> for DeclarationBodyRepr {
    fn from(body: &DeclarationBody) -> Self {
        match body {
            DeclarationBody::Nominal { representation } => Self::Nominal {
                representation: Box::new(representation.into()),
            },
            DeclarationBody::Alias { target } => Self::Alias {
                target: Box::new(target.into()),
            },
            DeclarationBody::Sum { constructors } => Self::Sum {
                constructors: constructors
                    .iter()
                    .map(|constructor| SumConstructorRepr {
                        name: constructor.name.clone(),
                        payload: constructor
                            .payload
                            .as_ref()
                            .map(|payload| Box::new(payload.into())),
                    })
                    .collect(),
            },
        }
    }
}

impl From<&Declaration> for DeclarationView {
    fn from(declaration: &Declaration) -> Self {
        Self {
            name: declaration.coordinate().name.clone(),
            body: declaration.body().into(),
            rendered: declaration.body().to_string().into(),
            digest: declaration.digest().digest().to_string().into(),
            // Doc text is not part of a declaration; it rides the
            // position-carrying sidecar (decision 0061), so the driver that
            // holds the sidecar fills this in.
            documentation: Box::from(""),
        }
    }
}

/// Why a declaration could not be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclarationError {
    /// The module already declares this name. The table's key is the pair, so
    /// this is a collision within one module and never across two.
    DuplicateName { module: Box<str>, name: Box<str> },
    /// The name is empty or contains the coordinate separator.
    InvalidName { module: Box<str>, name: Box<str> },
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
            Self::InvalidName { module, name } => write!(
                f,
                "`{name}` cannot be declared in module `{module}`: a declared name is the half \
                 after the dot in `{module}.<name>`, so it must be non-empty and dot-free"
            ),
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
    /// name, and — reported under the same variant, with the constructor as the
    /// name — when two constructors share one. There is no free sum constructor
    /// to refuse a repeat any more, so this is where constructors are sorted and
    /// a repeat is caught.
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
        if name.is_empty() || name.contains('.') {
            return Err(DeclarationError::InvalidName {
                module: self.module.clone(),
                name: name.into(),
            });
        }
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

    /// Encode the module and declarations in name order.
    #[must_use]
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut manifest = Vec::new();
        encode_str(&mut manifest, &self.module);
        encode_length(&mut manifest, self.entries.len());
        for entry in &self.entries {
            encode_bytes(&mut manifest, &entry.encode_canonical());
        }
        manifest
    }

    /// Decode one table from its canonical encoding. Entries must arrive in
    /// the name order the table maintains, and their coordinates must carry
    /// this table's module.
    ///
    /// # Errors
    /// Returns the reader's error, plus [`CanonicalDecodeError::NamesOutOfOrder`]
    /// for a table whose entries are not in strictly ascending name order.
    pub fn decode_canonical(encoded: &[u8]) -> Result<Self, CanonicalDecodeError> {
        let mut decoder = CanonicalReader::new(encoded);
        let module: Box<str> = decoder.read_text()?.into();
        let entries = decoder
            .read_sequence(|decoder| Declaration::decode_canonical(decoder.read_bytes()?))?;
        decoder.finish()?;
        for entry in &entries {
            if entry.coordinate.module != module {
                return Err(CanonicalDecodeError::DeclarationModuleMismatch {
                    table: module,
                    declaration: entry.coordinate.module.clone(),
                });
            }
        }
        for [earlier, later] in entries.array_windows() {
            if earlier.coordinate.name >= later.coordinate.name {
                return Err(CanonicalDecodeError::NamesOutOfOrder {
                    earlier: earlier.coordinate.name.clone(),
                    later: later.coordinate.name.clone(),
                });
            }
        }
        Ok(Self {
            module,
            entries: entries.into(),
        })
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
    fn the_kind_participates_in_the_digest() {
        // A nominal over `Blob` and an alias for `Blob` have the same body and
        // must not have the same digest: the kind is part of what the
        // declaration says, and the two are not interchangeable at a use site.
        let mut nominal = table();
        nominal.nominal("Thing", Type::Blob).unwrap();
        let mut alias = table();
        alias.alias("Thing", Type::Blob).unwrap();
        assert_ne!(
            nominal.get("Thing").unwrap().digest(),
            alias.get("Thing").unwrap().digest()
        );

        // And a sum whose single constructor carries `Blob` is a third thing.
        let mut sum = table();
        sum.sum(
            "Thing",
            [SumConstructor {
                name: "Only".into(),
                payload: Some(Type::Blob),
            }],
        )
        .unwrap();
        assert_ne!(
            nominal.get("Thing").unwrap().digest(),
            sum.get("Thing").unwrap().digest()
        );
    }

    #[test]
    fn a_constructor_payload_type_participates_and_a_reordering_does_not() {
        let build = |payloads: [Option<Type>; 2], reversed: bool| {
            let mut table = table();
            let mut constructors = [
                SumConstructor {
                    name: "A".into(),
                    payload: payloads[0].clone(),
                },
                SumConstructor {
                    name: "B".into(),
                    payload: payloads[1].clone(),
                },
            ];
            if reversed {
                constructors.reverse();
            }
            table.sum("S", constructors).unwrap();
            table.get("S").unwrap().digest()
        };
        // Construction order is not content; the payload types are.
        assert_eq!(
            build([Some(Type::Blob), Some(Type::Text)], false),
            build([Some(Type::Blob), Some(Type::Text)], true)
        );
        assert_ne!(
            build([Some(Type::Blob), Some(Type::Text)], false),
            build([Some(Type::Text), Some(Type::Blob)], false)
        );
        // Presence of a payload is content too.
        assert_ne!(
            build([Some(Type::Blob), None], false),
            build([Some(Type::Blob), Some(Type::Blob)], false)
        );
    }

    #[test]
    fn a_recursive_nominals_digest_is_finite_and_distinguishes_its_shape() {
        // The cut is what makes this terminate at all; that it also discriminates
        // is what keeps a recursive declaration's revision honest.
        let build = |body: Type| {
            let mut table = table();
            table.nominal("Tree", body).unwrap();
            table.get("Tree").unwrap().digest()
        };
        let over_list = build(Type::List(Box::new(Type::Cut)));
        assert_eq!(over_list, build(Type::List(Box::new(Type::Cut))));
        assert_ne!(over_list, build(Type::List(Box::new(Type::Blob))));
        assert_ne!(over_list, build(Type::Cut));
    }

    #[test]
    fn a_coordinate_spelling_round_trips_through_parse() {
        for (module, name) in [
            ("xylem", "Object"),
            ("", "bare"),
            ("a", "b"),
            ("phloem", "VersionScheme"),
        ] {
            let coordinate = Coordinate::new(module, name);
            assert_eq!(
                Coordinate::parse(&coordinate.spelling()),
                coordinate,
                "`{module}`.`{name}` did not survive the round trip"
            );
        }
    }

    #[test]
    fn an_alias_target_may_reference_a_declaration_from_the_same_table() {
        // The table admits a declaration whose body names an earlier one, which
        // is how a domain builds a compound type. Only *itself* is refused.
        let mut table = table();
        let object = table.nominal("Object", Type::Blob).unwrap();
        assert!(table.alias("Objects", Type::List(Box::new(object))).is_ok());
        assert_eq!(table.len(), 2);
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

    #[test]
    fn an_empty_or_dotted_name_is_refused() {
        let mut table = table();
        for name in ["", "a.b", ".hidden", "trailing."] {
            assert_eq!(
                table.nominal(name, Type::Text),
                Err(DeclarationError::InvalidName {
                    module: "test".into(),
                    name: name.into(),
                }),
                "`{name}` was accepted as a declared name"
            );
        }
    }

    #[test]
    fn a_tables_encoding_is_registration_order_free_and_self_delimiting() {
        let mut one = table();
        one.nominal("A", Type::Blob).unwrap();
        one.nominal("B", Type::Text).unwrap();
        let mut two = table();
        two.nominal("B", Type::Text).unwrap();
        two.nominal("A", Type::Blob).unwrap();
        assert_eq!(one.encode_canonical(), two.encode_canonical());

        let mut elsewhere = DeclarationTable::new("other");
        elsewhere.nominal("A", Type::Blob).unwrap();
        assert_ne!(one.encode_canonical(), elsewhere.encode_canonical());
        assert_ne!(
            one.encode_canonical(),
            table().encode_canonical(),
            "an empty table must differ from a populated one"
        );
    }

    #[test]
    fn a_digest_is_the_domain_hash_of_the_canonical_encoding() {
        let mut table = table();
        table.nominal("CSource", Type::Blob).unwrap();
        let declaration = table.get("CSource").unwrap();
        assert_eq!(
            declaration.digest(),
            pith_ids::DeclarationDigest::of_manifest(&declaration.encode_canonical())
        );
    }

    #[test]
    fn declaration_and_table_encodings_match_the_golden_bytes() {
        let mut table = table();
        assert!(table.nominal("A", Type::Text).is_ok());
        let Some(declaration) = table.get("A") else {
            return;
        };
        let declaration_bytes = [
            4, 0, 0, 0, 0, 0, 0, 0, b't', b'e', b's', b't', 1, 0, 0, 0, 0, 0, 0, 0, b'A', 0, 3,
        ];
        assert_eq!(declaration.encode_canonical(), declaration_bytes);

        let mut table_bytes = vec![4, 0, 0, 0, 0, 0, 0, 0, b't', b'e', b's', b't'];
        table_bytes.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
        table_bytes.extend_from_slice(&[23, 0, 0, 0, 0, 0, 0, 0]);
        table_bytes.extend_from_slice(&declaration_bytes);
        assert_eq!(table.encode_canonical(), table_bytes);
        assert_eq!(
            declaration.digest().digest().to_string(),
            "9625090b93578f90d87b6c3f0cd6da2f1bc27fca26262d78f0c8991b298b9359"
        );
    }

    fn populated_table() -> DeclarationTable {
        let mut table = table();
        table.nominal("CSource", Type::Blob).unwrap();
        table
            .sum(
                "Format",
                [
                    SumConstructor {
                        name: "elf".into(),
                        payload: None,
                    },
                    SumConstructor {
                        name: "about".into(),
                        payload: Some(Type::Text),
                    },
                ],
            )
            .unwrap();
        table
            .alias("Sources", Type::List(Box::new(Type::Blob)))
            .ok();
        table
    }

    #[test]
    fn a_table_round_trips_through_its_canonical_encoding() {
        let table = populated_table();
        let decoded = DeclarationTable::decode_canonical(&table.encode_canonical()).unwrap();
        assert_eq!(decoded, table);
        assert_eq!(decoded.module(), "test");
    }

    #[test]
    fn a_decoded_table_refuses_entries_out_of_name_order() {
        let table = populated_table();
        let names = ["CSource", "Format", "Sources"];
        let mut reversed = Vec::new();
        encode_str(&mut reversed, "test");
        encode_length(&mut reversed, names.len());
        for name in names.iter().rev() {
            encode_bytes(&mut reversed, &table.get(name).unwrap().encode_canonical());
        }
        assert_eq!(
            DeclarationTable::decode_canonical(&reversed).unwrap_err(),
            CanonicalDecodeError::NamesOutOfOrder {
                earlier: "Sources".into(),
                later: "Format".into(),
            }
        );
    }

    #[test]
    fn a_declaration_with_an_unknown_kind_byte_is_refused() {
        let mut encoded = Vec::new();
        encode_str(&mut encoded, "test");
        encode_str(&mut encoded, "A");
        encoded.push(9);
        assert_eq!(
            Declaration::decode_canonical(&encoded),
            Err(CanonicalDecodeError::UnknownDeclarationKind { kind: 9 })
        );
    }

    #[test]
    fn a_decoded_table_refuses_a_declaration_from_another_module() {
        let mut declaration_table = DeclarationTable::new("other");
        assert!(declaration_table.nominal("A", Type::Text).is_ok());
        let Some(declaration) = declaration_table.get("A") else {
            unreachable!("the declaration was inserted");
        };
        let mut encoded = Vec::new();
        encode_str(&mut encoded, "test");
        encode_length(&mut encoded, 1);
        encode_bytes(&mut encoded, &declaration.encode_canonical());

        assert_eq!(
            DeclarationTable::decode_canonical(&encoded),
            Err(CanonicalDecodeError::DeclarationModuleMismatch {
                table: "test".into(),
                declaration: "other".into(),
            })
        );
    }

    #[test]
    fn a_decoded_declaration_refuses_invalid_names_and_recursive_aliases() {
        let mut invalid_name = Vec::new();
        encode_str(&mut invalid_name, "test");
        encode_str(&mut invalid_name, "not.valid");
        invalid_name.push(0);
        encode_type_manifest(&mut invalid_name, &Type::Text);
        assert_eq!(
            Declaration::decode_canonical(&invalid_name),
            Err(CanonicalDecodeError::InvalidDeclarationName {
                name: "not.valid".into()
            })
        );

        let mut recursive_alias = Vec::new();
        encode_str(&mut recursive_alias, "test");
        encode_str(&mut recursive_alias, "Loop");
        recursive_alias.push(2);
        encode_type_manifest(&mut recursive_alias, &Type::List(Box::new(Type::Cut)));
        assert_eq!(
            Declaration::decode_canonical(&recursive_alias),
            Err(CanonicalDecodeError::RecursiveAlias {
                module: "test".into(),
                name: "Loop".into(),
            })
        );
    }
}
