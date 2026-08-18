//! The user merge: account contributions overlay into one user table.

use pith_core::{BodyRevision, Pure, Rule, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{PureRule, PureRuleFrame};

use crate::merge::{self, Keyed};
use crate::rules::{Leaf, contributions_of, diag, user_entries_of};
use crate::types::{self, MODULE};

/// Merge user contributions into one user table, refusing one account that
/// two owners declare differently.
pub struct ComposeUsers;

impl ComposeUsers {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::declared(
            MODULE,
            types::COMPOSE_USERS,
            BodyRevision(1),
            types::users_interface(),
            Span::none(),
        )
    }

    fn compute(inputs: &[Value]) -> PithResult<Value> {
        let Some(contributions) = inputs.first() else {
            return Err(diag(&format!(
                "a compose-users request supplies its contributions; this one supplied {}",
                inputs.len()
            )));
        };
        let contributions = contributions_of(contributions, types::USERS)?;
        let mut keyed = Vec::new();
        for (owner, table) in contributions {
            for (name, record) in user_entries_of(&table)? {
                keyed.push(Keyed {
                    owner: owner.clone(),
                    key: name,
                    value: record,
                });
            }
        }
        let merged = merge::merge_keyed("user account", &keyed)?;
        Ok(types::user_table_of(merged))
    }
}

impl PureRule for ComposeUsers {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(Leaf::new(inputs, Self::compute))
    }
}
