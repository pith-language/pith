//! Metamorphic route tests: independent spellings, canonical sources, content, and graph order.

mod build;
mod check;
mod model;
mod store;

use pith_loader::FrontendCode;
use proptest::prelude::*;

use build::build;
use model::{Change, Kind, Naming, cases};

proptest! {
    #[test]
    fn agreeing_graphs_preserve_content_and_abi_across_permutations(case in cases()) {
        for kind in [Kind::Path, Kind::Git, Kind::Archive, Kind::Registry] {
            let (original, reads) = build(&case, kind, &case.order(), Change::None, Naming::default()).run();
            let workspace = original.map_err(|diag| TestCaseError::fail(format!("valid graph refused: {diag:?}")))?;
            check::accepted(&case, &workspace, &reads)?;
            let expected = check::semantics(&workspace)?;
            for order in [case.order(), case.shuffled(), case.order().into_iter().rev().collect()] {
              for naming in Naming::variations() {
                let (changed, reads) = build(&case, kind, &order, Change::None, naming).run();
                let workspace = changed.map_err(|diag| TestCaseError::fail(format!("transformed graph refused: {diag:?}")))?;
                check::accepted(&case, &workspace, &reads)?;
                prop_assert_eq!(check::semantics(&workspace)?, expected.clone());
              }
            }
        }
    }

    #[test]
    fn distinct_sources_refuse_even_when_their_bytes_and_requests_match(mut case in cases()) {
        let chosen = case.chosen();
        for same_bytes in [false, true] {
            if same_bytes { case.alternate = case.content.clone(); }
            for kind in [Kind::Path, Kind::Git, Kind::Archive, Kind::Registry] {
                for order in [case.order(), case.order().into_iter().rev().collect(), case.shuffled()] {
                    for naming in Naming::variations() {
                        let (result, _) = build(&case, kind, &order, Change::Source(chosen), naming).run();
                        check::refused(result, FrontendCode::ConflictingRoutes)?;
                    }
                }
            }
        }
    }

    #[test]
    fn unavailable_routes_refuse_at_any_traversal_position(case in cases()) {
        let chosen = case.chosen();
        let others: Vec<_> = case.order().into_iter().filter(|index| *index != chosen).collect();
        let first = std::iter::once(chosen).chain(others.iter().copied()).collect::<Vec<_>>();
        let last = others.into_iter().chain(std::iter::once(chosen)).collect::<Vec<_>>();
        for kind in [Kind::Path, Kind::Git, Kind::Archive, Kind::Registry] {
            for order in [&first, &last] {
                for naming in Naming::variations() {
                    let (result, _) = build(&case, kind, order, Change::Unreachable(chosen), naming).run();
                    check::refused(result, FrontendCode::MissingManifest)?;
                }
            }
        }
    }

    #[test]
    fn alias_and_order_changes_cannot_erase_a_changed_trust_root(case in cases()) {
        for order in [case.order(), case.order().into_iter().rev().collect(), case.shuffled()] {
            for naming in Naming::variations() {
                let (result, _) = build(&case, Kind::Registry, &order, Change::Trust(case.chosen()), naming).run();
                check::refused(result, FrontendCode::ConflictingRoutes)?;
            }
        }
    }
}
