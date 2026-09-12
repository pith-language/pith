//! The merge operator in this library.
//!
//! Records compose field by field under a declared policy; keyed collections
//! (file sets by path, user tables by account name) compose by key. Both fail
//! closed: agreeing contributions collapse, a key or field naming two values
//! is a diagnostic naming both owners, and a replacement naming the field and
//! its expected owner is the only way a value wins. Inputs are sorted before
//! anything is compared, so the result is a function of the contribution set
//! plus the policy.

use pith_core::{RecordField, Value};
use pith_diag::PithResult;

use crate::rules::diag;
use crate::types::Behavior;

/// One contribution to a record merge: who declares it, and the record they
/// declare.
#[derive(Clone, Debug)]
pub struct Contribution {
    pub owner: Box<str>,
    pub value: Value,
}

/// One keyed contribution: the owner, the key the entry lands under, and the
/// value at that key.
#[derive(Clone, Debug)]
pub struct Keyed {
    pub owner: Box<str>,
    pub key: Box<str>,
    pub value: Value,
}

/// Merge record contributions under `policy`.
///
/// A field the policy does not name must agree, so accumulation has to be
/// declared at the merge site. A `concat` field concatenates every
/// contribution's list and canonicalizes the result to sorted,
/// duplicate-free order.
///
/// # Errors
/// A diagnostic when two contributions disagree on a field that must agree,
/// when a contribution's value is not a record, or when a concat field does
/// not carry a list.
pub fn merge_records(
    what: &str,
    policy: &[(Box<str>, Behavior)],
    contributions: &[Contribution],
) -> PithResult<Value> {
    let mut contributions: Vec<&Contribution> = contributions.iter().collect();
    contributions.sort_by(|left, right| left.owner.cmp(&right.owner));

    let mut fields: Vec<Box<str>> = Vec::new();
    for contribution in &contributions {
        let Value::Record(record) = &contribution.value else {
            return Err(diag(&format!(
                "the contribution from `{}` to a merged {what} was {}, not a record",
                contribution.owner,
                contribution.value.describe()
            )));
        };
        for field in record {
            if !fields
                .iter()
                .any(|known| known.as_ref() == field.name.as_ref())
            {
                fields.push(field.name.clone());
            }
        }
    }
    fields.sort();

    let merged = fields
        .iter()
        .map(|field| {
            let carriers: Vec<&Contribution> = contributions
                .iter()
                .filter(|contribution| record_field(&contribution.value, field).is_some())
                .copied()
                .collect();
            let behavior = policy
                .iter()
                .find(|(named, _)| named == field)
                .map(|(_, behavior)| *behavior);
            let value = match behavior {
                None | Some(Behavior::Agree) => {
                    agree(what, field, &carriers)?;
                    carriers
                        .first()
                        .and_then(|carrier| record_field(&carrier.value, field))
                        .cloned()
                        .unwrap_or_else(|| {
                            unreachable!("a carrier was found for a field it carries")
                        })
                }
                Some(Behavior::Concat) => concat(what, field, &carriers)?,
            };
            Ok((field.clone(), value))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let record = merged
        .into_iter()
        .map(|(name, payload)| RecordField { name, payload })
        .collect::<Vec<RecordField<Value>>>();
    Value::record(record).map_err(|error| {
        diag(&format!(
            "merging a {what} produced an invalid record: {error}"
        ))
    })
}

/// Check that every carrier agrees on `field`, or produce the diagnostic
/// naming the first disagreement with both owners.
fn agree(what: &str, field: &str, carriers: &[&Contribution]) -> PithResult<()> {
    let Some(first) = carriers.first() else {
        return Ok(());
    };
    let first_value = record_field(&first.value, field)
        .unwrap_or_else(|| unreachable!("a carrier was found for a field it carries"));
    for carrier in carriers.iter().skip(1) {
        let value = record_field(&carrier.value, field)
            .unwrap_or_else(|| unreachable!("a carrier was found for a field it carries"));
        if value != first_value {
            return Err(diag(&format!(
                "the `{field}` of a merged {what} is `{}` from `{}` and `{}` from `{}`, and \
                 neither replaces the other",
                first_value.describe(),
                first.owner,
                value.describe(),
                carrier.owner,
            )));
        }
    }
    Ok(())
}

/// Every carrier's list under `field`, concatenated and canonicalized.
fn concat(what: &str, field: &str, carriers: &[&Contribution]) -> PithResult<Value> {
    let mut items: Vec<Value> = Vec::new();
    for carrier in carriers {
        let value = record_field(&carrier.value, field)
            .unwrap_or_else(|| unreachable!("a carrier was found for a field it carries"));
        let Value::List(elements) = value else {
            return Err(diag(&format!(
                "the `{field}` of a merged {what} is concatenated, so the contribution from \
                 `{}` must carry a list, not {}",
                carrier.owner,
                value.describe()
            )));
        };
        items.extend(elements.iter().cloned());
    }
    Ok(crate::types::canonical_list(items))
}

/// Replace `field` with `value`, as `expected_owner` declares it: the one
/// explicit way a value wins. Fails when `expected_owner` no longer declares
/// the field, so a replacement is held to the ownership it named. On success
/// the value lands on every carrier.
///
/// # Errors
/// A diagnostic naming the field's current owners, when `expected_owner` is
/// not among them or nobody declares the field.
pub fn replace_field(
    what: &str,
    contributions: &[Contribution],
    field: &str,
    expected_owner: &str,
    value: Value,
) -> PithResult<Vec<Contribution>> {
    let owners: Vec<&str> = contributions
        .iter()
        .filter(|contribution| record_field(&contribution.value, field).is_some())
        .map(|contribution| contribution.owner.as_ref())
        .collect();
    if owners.is_empty() {
        return Err(diag(&format!(
            "`{expected_owner}` replaces the `{field}` of a merged {what}, but no contribution \
             declares that field"
        )));
    }
    if !owners.contains(&expected_owner) {
        let mut spellings: Vec<String> = owners.iter().map(|owner| format!("`{owner}`")).collect();
        spellings.sort();
        return Err(diag(&format!(
            "`{expected_owner}` replaces the `{field}` of a merged {what} as if it still \
             declared it, but it is now declared by {}",
            spellings.join(" and ")
        )));
    }

    contributions
        .iter()
        .map(|contribution| {
            let mut replaced = contribution.clone();
            if record_field(&contribution.value, field).is_some() {
                let Value::Record(record) = &contribution.value else {
                    return Err(diag(&format!(
                        "the contribution from `{}` to a merged {what} was {}, not a record",
                        contribution.owner,
                        contribution.value.describe()
                    )));
                };
                let mut fields: Vec<(Box<str>, Value)> = record
                    .iter()
                    .map(|entry| (entry.name.clone(), entry.payload.clone()))
                    .collect();
                match fields.iter_mut().find(|(name, _)| name.as_ref() == field) {
                    Some((_, payload)) => *payload = value.clone(),
                    None => {
                        return Err(diag(&format!(
                            "`{}` does not carry the `{field}` a replacement names",
                            contribution.owner
                        )));
                    }
                }
                fields.sort_by(|(left, _), (right, _)| left.cmp(right));
                let record: Vec<RecordField<Value>> = fields
                    .into_iter()
                    .map(|(name, payload)| RecordField { name, payload })
                    .collect();
                replaced.value = Value::record(record).map_err(|error| {
                    diag(&format!("a replaced {what} record is invalid: {error}"))
                })?;
            }
            Ok(replaced)
        })
        .collect()
}

/// Merge keyed contributions: agreeing entries collapse, and one key naming
/// two values is a diagnostic naming both owners. The result is sorted by
/// key, so it is a function of the set of entries.
///
/// # Errors
/// A diagnostic naming the key and both owners when two contributions
/// disagree on one key's value.
pub fn merge_keyed(what: &str, entries: &[Keyed]) -> PithResult<Vec<(Box<str>, Value)>> {
    let mut entries: Vec<&Keyed> = entries.iter().collect();
    entries.sort_by(|left, right| {
        (left.key.as_ref(), left.owner.as_ref()).cmp(&(right.key.as_ref(), right.owner.as_ref()))
    });

    let mut merged: Vec<(Box<str>, Value)> = Vec::new();
    let mut iterator = entries.into_iter().peekable();
    while let Some(first) = iterator.next() {
        let key = first.key.clone();
        let value = first.value.clone();
        while iterator.peek().is_some_and(|next| next.key == key) {
            let next = match iterator.next() {
                Some(next) => next,
                None => unreachable!("an entry was peeked and is still there"),
            };
            if next.value != value {
                return Err(diag(&format!(
                    "the {what} `{}` is {} from `{}` and {} from `{}`, and neither replaces \
                     the other",
                    key,
                    value.describe(),
                    first.owner,
                    next.value.describe(),
                    next.owner,
                )));
            }
        }
        merged.push((key, value));
    }
    Ok(merged)
}

fn record_field<'a>(value: &'a Value, field: &str) -> Option<&'a Value> {
    let Value::Record(record) = value else {
        return None;
    };
    record
        .iter()
        .find(|entry| entry.name.as_ref() == field)
        .map(|entry| &entry.payload)
}
