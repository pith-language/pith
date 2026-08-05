use pith_core::CapabilityRequirement;

use super::{DependencyEdge, Engine};

pub(super) fn canonical_capabilities<'capability>(
    capabilities: impl IntoIterator<Item = &'capability CapabilityRequirement>,
) -> Box<[CapabilityRequirement]> {
    let mut capabilities: Vec<_> = capabilities.into_iter().cloned().collect();
    capabilities.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.scope.cmp(&right.scope))
    });
    capabilities.dedup();
    capabilities.into_boxed_slice()
}

impl Engine {
    pub(super) fn effective_capabilities(
        &self,
        dependencies: &[DependencyEdge],
    ) -> Option<Box<[CapabilityRequirement]>> {
        let dependency_capabilities = dependencies
            .iter()
            .filter_map(DependencyEdge::computation_id)
            .map(|computation| {
                self.computations
                    .get(computation)
                    .map(|node| node.capabilities.iter())
            })
            .collect::<Option<Vec<_>>>()?;
        Some(canonical_capabilities(
            dependency_capabilities.into_iter().flatten(),
        ))
    }
}
