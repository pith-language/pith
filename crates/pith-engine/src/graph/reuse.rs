use indexmap::IndexMap;
use pith_core::{Interface, Pure, Request, RuleId, Value};
use pith_ids::ComputationId;

use super::{DependencyEdge, Engine, Evaluation, EvaluationSource};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct PureApplicationKey {
    rule: RuleId,
    interface: Interface,
    inputs: Box<[Value]>,
}

impl PureApplicationKey {
    pub(super) fn new(rule: RuleId, request: &Request<Pure>) -> Self {
        Self {
            rule,
            interface: request.interface.clone(),
            inputs: request.inputs.clone(),
        }
    }
}

impl Engine {
    pub(super) fn reusable_pure_evaluation(
        &self,
        rule: RuleId,
        request: &Request<Pure>,
    ) -> Option<Evaluation> {
        let key = PureApplicationKey::new(rule, request);
        let computation = *self.pure_computations.get(&key)?;
        let node = self.computations.get(computation)?;
        if !node.is_reusable {
            return None;
        }
        let value = node.result.clone()?;
        Some(Evaluation {
            value,
            computation,
            source: EvaluationSource::Reused,
        })
    }

    pub(super) fn index_pure_computation(
        &mut self,
        rule: RuleId,
        request: &Request<Pure>,
        computation: ComputationId,
    ) {
        let key = PureApplicationKey::new(rule, request);
        self.pure_computations.insert(key, computation);
    }

    pub(super) fn pure_dependencies_are_reusable(&self, dependencies: &[DependencyEdge]) -> bool {
        dependencies.iter().all(|dependency| match dependency {
            DependencyEdge::Blob { .. } => true,
            DependencyEdge::Request { computation, .. } => self
                .computations
                .get(*computation)
                .is_some_and(|node| node.is_reusable),
            DependencyEdge::Action { .. } => false,
        })
    }
}

pub(super) type PureComputationIndex = IndexMap<PureApplicationKey, ComputationId>;
