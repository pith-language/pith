//! Property tests binding indexed selection to the scan it replaced
//! (decision 0057).
//!
//! The index is a view of the rule population, so the contract is agreement:
//! for any population and any request, the table answers what a linear scan
//! comparing every rule's interface would have answered — the same rule when
//! one matches, the same candidates in the same order when several do, and no
//! match when none does. The reference below is the pre-0057 implementation,
//! kept here so the equivalence is executable rather than argued.

use pith_core::{
    DeclarationTable, Interface, Pure, Request, Rule, RuleId, RuleTable, SelectOutcome, Type,
};
use pith_diag::Span;
use pith_ids::{RuleIdentity, RuleRevision};
use proptest::prelude::*;
use smallvec::SmallVec;

/// The types interfaces are drawn from. Two, so that a population of a dozen
/// rules over fourteen possible interfaces collides often: agreement on the
/// ambiguous branch is the half of the contract a wider pool would rarely
/// generate.
#[allow(
    clippy::expect_used,
    reason = "fixture setup; a pool this test cannot declare has nothing to select over"
)]
fn type_pool() -> Vec<Type> {
    let mut declarations = DeclarationTable::new("pith-core.selection-properties");
    let nominal = declarations
        .nominal("Handle", Type::Blob)
        .expect("the pool declares each name once");
    vec![Type::Int, nominal]
}

/// Labels are drawn from a pool for the same reason: two rules sharing one
/// label is what makes the candidate ordering's tie-break observable.
const LABEL_POOL: [&str; 3] = ["alpha", "beta", "gamma"];

fn interface_strategy(pool: Vec<Type>) -> impl Strategy<Value = Interface> {
    let size = pool.len();
    (
        proptest::collection::vec(0..size, 0..3),
        proptest::sample::select((0..size).collect::<Vec<_>>()),
    )
        .prop_map(move |(inputs, output)| {
            let pick = |index: usize| {
                pool.get(index)
                    .cloned()
                    .unwrap_or_else(|| unreachable!("indices are drawn from the pool's range"))
            };
            Interface {
                inputs: inputs.into_iter().map(pick).collect(),
                output: pick(output),
            }
        })
}

fn population_strategy() -> impl Strategy<Value = Vec<(usize, Interface)>> {
    proptest::collection::vec(
        (0..LABEL_POOL.len(), interface_strategy(type_pool())),
        0..12,
    )
}

/// Selection as it was before the index: compare every registered rule's
/// interface with the request's, then order the survivors canonically.
fn scan<'table>(table: &'table RuleTable<Pure>, request: &Request<Pure>) -> SelectOutcome {
    let mut candidates: Vec<(&'table Interface, &'table str, RuleId)> = table
        .iter()
        .filter(|(_, rule)| rule.interface == request.interface)
        .map(|(id, rule)| (&rule.interface, rule.label.as_ref(), id))
        .collect();
    candidates.sort_by(|left, right| left.0.cmp(right.0).then_with(|| left.1.cmp(right.1)));

    let candidates: SmallVec<[RuleId; 2]> = candidates.into_iter().map(|(_, _, id)| id).collect();
    match candidates.as_slice() {
        [] => SelectOutcome::NoMatch,
        [only] => SelectOutcome::One(*only),
        _ => SelectOutcome::Ambiguous(candidates),
    }
}

fn table_of(population: &[(usize, Interface)]) -> RuleTable<Pure> {
    let mut table = RuleTable::new();
    for (label, interface) in population {
        let label = LABEL_POOL
            .get(*label)
            .unwrap_or_else(|| unreachable!("labels are drawn from the pool's range"));
        let identity = RuleIdentity::of_module_declaration("pith-core.selection-properties", label);
        let revision = RuleRevision::of_manifest(identity, b"selection-properties");
        table.push(Rule::<Pure>::new(
            "pith-core.selection-properties",
            revision,
            *label,
            interface.clone(),
            Span::none(),
        ));
    }
    table
}

proptest! {
    /// The index answers what the scan answered, for a requested interface that
    /// is usually in the population.
    #[test]
    fn indexed_selection_matches_a_scan(
        population in population_strategy(),
        requested in interface_strategy(type_pool()),
    ) {
        let table = table_of(&population);
        let request = Request::<Pure>::new("requested", requested, [], Span::none());
        prop_assert_eq!(table.select(&request), scan(&table, &request));
    }

    /// The same agreement when the request names an interface no rule provides:
    /// an absent key is a miss, not a bucket that happens to be empty.
    #[test]
    fn an_unprovided_interface_misses_in_both(population in population_strategy()) {
        let table = table_of(&population);
        let unprovided = Interface {
            inputs: vec![Type::Bool].into_boxed_slice(),
            output: Type::Bytes,
        };
        let request = Request::<Pure>::new("unprovided", unprovided, [], Span::none());
        prop_assert_eq!(table.select(&request), SelectOutcome::NoMatch);
        prop_assert_eq!(scan(&table, &request), SelectOutcome::NoMatch);
    }

    /// Registration order changes ids but not the outcome's shape: reversing the
    /// population selects the rule with the same label, and reports the same
    /// candidate labels in the same order (decision 0015).
    #[test]
    fn reversing_the_population_selects_the_same_labels(
        population in population_strategy(),
        requested in interface_strategy(type_pool()),
    ) {
        let mut reversed = population.clone();
        reversed.reverse();
        let forward = table_of(&population);
        let backward = table_of(&reversed);
        let request = Request::<Pure>::new("requested", requested, [], Span::none());
        prop_assert_eq!(
            labels(&forward, &forward.select(&request)),
            labels(&backward, &backward.select(&request))
        );
    }
}

/// The labels an outcome names, which is what a diagnostic renders and the only
/// part of it that does not depend on registration order.
fn labels(table: &RuleTable<Pure>, outcome: &SelectOutcome) -> Vec<String> {
    let ids: Vec<RuleId> = match outcome {
        SelectOutcome::NoMatch => Vec::new(),
        SelectOutcome::One(id) => vec![*id],
        SelectOutcome::Ambiguous(candidates) => candidates.to_vec(),
    };
    ids.into_iter()
        .filter_map(|id| table.get(id).map(|rule| rule.label.to_string()))
        .collect()
}
