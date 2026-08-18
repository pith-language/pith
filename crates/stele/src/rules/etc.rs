//! The /etc merge: file contributions overlay into one file set.

use pith_core::{BodyRevision, Pure, Rule, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{PureRule, PureRuleFrame};

use crate::merge::{self, Keyed};
use crate::rules::{Leaf, contributions_of, diag, file_entries_of};
use crate::types::{self, MODULE};

/// Merge file contributions into one file set, refusing one path that names
/// two bodies.
pub struct ComposeEtc;

impl ComposeEtc {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::declared(
            MODULE,
            types::COMPOSE_ETC,
            BodyRevision(1),
            types::etc_interface(),
            Span::none(),
        )
    }

    fn compute(inputs: &[Value]) -> PithResult<Value> {
        let Some(contributions) = inputs.first() else {
            return Err(diag(&format!(
                "a compose-etc request supplies its contributions; this one supplied {}",
                inputs.len()
            )));
        };
        let contributions = contributions_of(contributions, types::FILES)?;
        let mut keyed = Vec::new();
        for (owner, files) in contributions {
            for (path, body) in file_entries_of(&files)? {
                keyed.push(Keyed {
                    owner: owner.clone(),
                    key: path,
                    value: types::file_body_value(&body),
                });
            }
        }
        let merged = merge::merge_keyed("file path", &keyed)?;
        Ok(types::file_set_of(merged))
    }
}

impl PureRule for ComposeEtc {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(Leaf::new(inputs, Self::compute))
    }
}
