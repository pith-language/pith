//! The unit merge: fragments compose under a declared policy, with deliberate
//! replacement as the only way a value wins.

use pith_core::{BodyRevision, Pure, Rule, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{PureRule, PureRuleFrame};

use crate::merge::{self, Contribution};
use crate::rules::{Leaf, contributions_of, diag, field_of, representation_of, text_of};
use crate::types::{self, Behavior, MODULE};

/// Merge unit contributions under the declared policy, applying each declared
/// replacement first and holding it to the owner it names.
pub struct ComposeUnit;

impl ComposeUnit {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::declared(
            MODULE,
            types::COMPOSE_UNIT,
            BodyRevision(1),
            types::unit_interface(),
            Span::none(),
        )
    }

    fn compute(inputs: &[Value]) -> PithResult<Value> {
        let [policy, contributions, replacements] = inputs else {
            return Err(diag(&format!(
                "a compose-unit request supplies a policy, contributions, and replacements; \
                 this one supplied {}",
                inputs.len()
            )));
        };
        let policy = policy_of(policy)?;
        let contributions = contributions_of(contributions, types::UNIT)?;
        let mut merged: Vec<Contribution> = Vec::with_capacity(contributions.len());
        for (owner, unit) in &contributions {
            let record = representation_of(unit, types::unit())?;
            merged.push(Contribution {
                owner: owner.clone(),
                value: record.clone(),
            });
        }

        for replacement in replacements_of(replacements)? {
            merged = merge::replace_field(
                "unit",
                &merged,
                &replacement.field,
                &replacement.expected_owner,
                Value::Text(replacement.value),
            )?;
        }

        let record = merge::merge_records("unit", &policy, &merged)?;
        Ok(types::unit().value(record))
    }
}

/// The behaviors a policy value names, in the sorted order the value carries.
fn policy_of(value: &Value) -> PithResult<Vec<(Box<str>, Behavior)>> {
    let representation = representation_of(value, types::unit_policy())?;
    let Value::List(items) = representation else {
        return Err(diag(&format!(
            "a {} value carried {} rather than a list",
            types::unit_policy().name(),
            representation.describe()
        )));
    };
    let mut policy = Vec::with_capacity(items.len());
    for item in items {
        let Some(field) = field_of(item, types::FIELD) else {
            return Err(diag("a policy entry was missing its field"));
        };
        let Some(behavior) = field_of(item, types::BEHAVIOR) else {
            return Err(diag("a policy entry was missing its behavior"));
        };
        let Value::Sum {
            type_name,
            constructor,
            ..
        } = behavior
        else {
            return Err(diag(&format!(
                "a policy behavior was {}, not a declared {}",
                behavior.describe(),
                types::field_behavior().name()
            )));
        };
        if type_name.as_ref() != types::field_behavior().name() {
            return Err(diag(&format!(
                "a policy behavior named {type_name}, not {}",
                types::field_behavior().name()
            )));
        }
        let behavior = match constructor.as_ref() {
            "agree" => Behavior::Agree,
            "concat" => Behavior::Concat,
            other => {
                return Err(diag(&format!(
                    "a policy behavior named the `{other}` constructor, which {} does not \
                     declare",
                    types::field_behavior().name()
                )));
            }
        };
        policy.push((text_of(field)?.into(), behavior));
    }
    Ok(policy)
}

struct Replacement {
    field: Box<str>,
    expected_owner: Box<str>,
    value: Box<str>,
}

fn replacements_of(value: &Value) -> PithResult<Vec<Replacement>> {
    let Value::List(items) = value else {
        return Err(diag(&format!(
            "expected a list of replacements, found {}",
            value.describe()
        )));
    };
    let mut replacements = Vec::with_capacity(items.len());
    for item in items {
        let Some(field) = field_of(item, types::FIELD) else {
            return Err(diag("a replacement was missing its field"));
        };
        let Some(expected_owner) = field_of(item, types::EXPECTED_OWNER) else {
            return Err(diag("a replacement was missing its expected owner"));
        };
        let Some(replacement) = field_of(item, types::VALUE) else {
            return Err(diag("a replacement was missing its value"));
        };
        replacements.push(Replacement {
            field: text_of(field)?.into(),
            expected_owner: text_of(expected_owner)?.into(),
            value: text_of(replacement)?.into(),
        });
    }
    Ok(replacements)
}

impl PureRule for ComposeUnit {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(Leaf::new(inputs, Self::compute))
    }
}
