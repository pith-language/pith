//! The read-only query interface over a built graph (requirement K-12).

use pith_core::{Pure, Request, Rule, RuleId, select_rule};
use pith_diag::Diag;
use pith_ids::ComputationId;

use super::Engine;
use super::ir::{ComputationNode, DependencyEdge, RuleSelection};

pub struct EngineQuery<'engine> {
    engine: &'engine Engine,
}

impl<'engine> EngineQuery<'engine> {
    /// # Errors
    /// Returns `E-1101`, `E-1102`, or `E-1103` when the request cannot select
    /// exactly one rule.
    pub fn select(&self, request: &Request<Pure>) -> Result<RuleSelection, Diag> {
        request.validate_inputs()?;
        let rule =
            select_rule(request, &self.engine.rules).into_result(request, &self.engine.rules)?;
        Ok(RuleSelection {
            rule,
            interface: request.interface.clone(),
        })
    }

    pub fn rules(&self) -> impl Iterator<Item = (RuleId, &'engine Rule<Pure>)> + 'engine {
        self.engine.rules.iter()
    }

    pub fn rule(&self, id: RuleId) -> Option<&'engine Rule<Pure>> {
        self.engine.rules.get(id)
    }

    pub fn computations(
        &self,
    ) -> impl Iterator<Item = (ComputationId, &'engine ComputationNode)> + 'engine {
        self.engine.computations.iter()
    }

    pub fn computation(&self, id: ComputationId) -> Option<&'engine ComputationNode> {
        self.engine.computations.get(id)
    }

    pub fn dependencies_of(&self, id: ComputationId) -> Option<&'engine [DependencyEdge]> {
        self.engine
            .computations
            .get(id)
            .map(|node| node.dependencies.as_slice())
    }

    pub fn dependents_of(
        &self,
        dependency: ComputationId,
    ) -> impl Iterator<Item = (ComputationId, &'engine DependencyEdge)> + 'engine {
        self.engine
            .computations
            .iter()
            .flat_map(move |(computation, node)| {
                node.dependencies
                    .iter()
                    .filter(move |edge| edge.computation_id() == Some(dependency))
                    .map(move |edge| (computation, edge))
            })
    }
}

impl Engine {
    pub fn query(&self) -> EngineQuery<'_> {
        EngineQuery { engine: self }
    }
}
