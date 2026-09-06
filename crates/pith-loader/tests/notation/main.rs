//! Written-body notation, request forms, local definitions, entries, and
//! metadata blocks.
//!
//! [`bodies`] holds the represented bodies and their evaluation, [`requests`]
//! the request forms and the positions that check them, and [`declarations`]
//! the module-level surface. All three load through the helpers here.

mod bodies;
mod declarations;
mod requests;

use pith_core::{Pure, Request, Value};
use pith_diag::SourceId;
use pith_loader::{FrontendCode, ImportEnv, LoadedModule, ModuleSource, load_module};
use proptest::prelude::*;

fn load(text: &str) -> Result<LoadedModule, Box<[pith_diag::Diag]>> {
    load_module(
        &ModuleSource::new("test", SourceId::from_raw(1), "test.pi", text),
        &ImportEnv::new(),
    )
}

fn load_ok(text: &str) -> LoadedModule {
    match load(text) {
        Ok(loaded) => loaded,
        Err(diagnostics) => unreachable!("the notation loads: {diagnostics:?}"),
    }
}

fn load_err(text: &str) -> Box<[pith_diag::Diag]> {
    match load(text) {
        Err(diagnostics) => diagnostics,
        Ok(_) => unreachable!("the notation refuses this module"),
    }
}

fn has_code(diagnostics: &[pith_diag::Diag], code: FrontendCode) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code.stable())
}

/// A request over a loaded rule's interface, for the evaluations below.
fn request_of(interface: &pith_core::Interface, inputs: &[Value]) -> Request<Pure> {
    Request::new(
        "notation",
        interface.clone(),
        inputs,
        pith_diag::Span::none(),
    )
}

/// The interface a loaded rule exposes.
fn interface_of<'a>(loaded: &'a LoadedModule, label: &str) -> &'a pith_core::Interface {
    loaded
        .represented_pure_rule(label)
        .unwrap_or_else(|| unreachable!("the rule elaborates"))
        .interface()
}

const EXPRESSIONS: &str = "sum Shape = circle(Int) | square

pure rule \"make-shape\"(n: Int) -> Shape = {
  if n == 0 { square() } else { circle(n * 2 - 1) }
}

pure rule area(s: Shape) -> Int = {
  match s {
    circle(radius) { radius * 3 }
    square { 14 }
  }
}

pure rule described(s: Shape) -> Text = {
  match s {
    circle(radius) { concat(describe(radius), \" sides\") }
    square { \"four sides\" }
  }
}

pure rule \"sum-of\"(xs: List<Int>) -> Int = {
  fold xs from 0 { (element, accumulator) -> element + accumulator }
}

";

proptest! {
    #[test]
    fn arbitrary_text_terminates_stably(text in any::<String>()) {
        let first = load(&text);
        let second = load(&text);
        prop_assert_eq!(first.is_ok(), second.is_ok());
        if let (Err(first), Err(second)) = (first, second) {
            let positions = |diagnostics: &[pith_diag::Diag]| {
                diagnostics
                    .iter()
                    .map(|diagnostic| (diagnostic.code, diagnostic.span))
                    .collect::<Vec<_>>()
            };
            prop_assert_eq!(positions(&first), positions(&second));
        }
    }
}
