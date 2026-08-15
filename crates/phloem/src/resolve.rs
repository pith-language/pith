//! The solver as a host rule in the graph (decision 0040).
//!
//! The body is a host rule on 0038's declared tier: the search is not
//! structurally recursive, so it has no represented spelling under 0018,
//! and it owes the engine only the determinism contract — the answer is a
//! pure function of the request under the ordering the request names. The
//! search's interior (backtracking, host maps keyed by subject) is
//! invisible to the engine, which is the same epistemic position it holds
//! toward every host rule; the value spellings stay canonically sorted
//! lists, whose sortedness is what the digest and the diff need.
//!
//! The declared ordering a resolution runs under is a request input, on the
//! terms xylem made the toolchain one: the `phloem.VersionScheme` value
//! carries the scheme's declared name, [`Schemes`] is the lookup table that
//! name resolves against, and because the name participates in the
//! computation key, a resolution under a different ordering is a different
//! computation the reusable index cannot cross-serve. A scheme held as bare
//! rule state would be the third class 0038 named, state neither the
//! request nor the revision covers, and an answer recorded under one
//! ordering would be served under another.

use pith_core::{Pure, Rule, RuleIdentity, RuleRevision, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{PureRule, PureRuleFrame, PureStep, Resumption};

use crate::constraint::Constraint;
use crate::identity::{
    DEBIAN, Debian, NUMERIC_SEGMENTS, NumericSegments, VersionScheme, version_scheme_name,
};
use crate::preference::preference_list_from_value;
use crate::resolution::resolve_interface;
use crate::search::{SolveRequest, resolve};
use crate::universe::CandidateUniverse;

/// The declared version orderings one registration serves, resolved by the
/// name a request carries. Mirrors xylem's `Toolchains`: one registration of
/// the rule serves every scheme, because the scheme value in the request is
/// what dispatch and the computation key see. Holding one scheme per
/// registration would mean one engine per ordering, and registering the rule
/// twice would give two rules one interface and collide as `E-1102`
/// (decision 0015).
#[derive(Default)]
pub struct Schemes {
    entries: Box<[(Box<str>, Scheme)]>,
}

type Scheme = Box<dyn VersionScheme + Send + Sync>;

impl Schemes {
    /// A set over `(declared name, scheme)` pairs, held in name order.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming a declared name when more than
    /// one ordering claims it.
    pub fn new(mut entries: Box<[(Box<str>, Scheme)]>) -> PithResult<Self> {
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        if let Some(duplicate) = entries.windows(2).find_map(|pair| match pair {
            [left, right] if left.0 == right.0 => Some(left.0.as_ref()),
            _ => None,
        }) {
            return Err(crate::diag(format!(
                "the version scheme `{duplicate}` was registered twice; one declared name \
                 cannot select two orderings"
            )));
        }
        Ok(Self { entries })
    }

    /// The two orderings phloem declares: the dot-separated numeric scheme
    /// and the Debian scheme with epochs and tilde ordering (0039).
    #[must_use]
    pub fn standard() -> Self {
        Self {
            entries: Box::new([
                (NUMERIC_SEGMENTS.into(), Box::new(NumericSegments)),
                (DEBIAN.into(), Box::new(Debian)),
            ]),
        }
    }

    /// The ordering a request's scheme name resolves to, or `None` when this
    /// registration does not carry it.
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<&Scheme> {
        self.entries
            .iter()
            .find(|(declared, _)| declared.as_ref() == name)
            .map(|(_, scheme)| scheme)
    }

    fn names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(|(declared, _)| declared.as_ref())
            .collect()
    }
}

/// The registered scheme the request's first input names, on the terms
/// xylem's `requested_toolchain` set: a name this registration does not
/// carry is a diagnostic naming the requested scheme and every registered
/// one.
fn requested_scheme<'a>(schemes: &'a Schemes, value: &Value) -> PithResult<&'a Scheme> {
    let name = version_scheme_name(value)?;
    schemes.resolve(name).ok_or_else(|| {
        crate::diag(format!(
            "the request named the version scheme `{name}`, which this solver was not \
             registered with; registered schemes: {}",
            schemes.names().join(", ")
        ))
    })
}

/// The solver's host-rule body: one step, completing with the answer value.
/// The engine never sees the search.
pub struct ResolveSolver {
    schemes: Schemes,
}

impl ResolveSolver {
    #[must_use]
    pub fn new(schemes: Schemes) -> Self {
        Self { schemes }
    }

    /// The rule, ready to register against the resolution interface.
    #[must_use]
    pub fn rule(&self) -> Rule<Pure> {
        let identity = RuleIdentity::of_module_declaration("phloem", "resolve");
        Rule::<Pure>::new(
            RuleRevision::of_manifest(identity, RESOLVER_MANIFEST),
            "resolve",
            resolve_interface(),
            Span::none(),
        )
    }
}

/// The revision manifest of the resolve rule. A lock records the digest of
/// the revision it was resolved under (0041), because the selection is a
/// function of the request under the revision as much as under the scheme,
/// and a lock whose entries moved while no recorded input moved has this
/// one explanation left to name.
const RESOLVER_MANIFEST: &[u8] = b"phloem-resolve-v1";

/// The resolver revision a lock records, as lowercase hex. The one spelling
/// of the rule revision every registered solver shares, so the lock names
/// the resolve rule's executable semantics and nothing about which solver
/// body served it.
#[must_use]
pub fn resolver_revision_hex() -> Box<str> {
    let identity = RuleIdentity::of_module_declaration("phloem", "resolve");
    RuleRevision::of_manifest(identity, RESOLVER_MANIFEST)
        .digest()
        .to_string()
        .into()
}

impl PureRule for ResolveSolver {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ResolveFrame {
            answer: Some(solve_from_values(&self.schemes, inputs)),
        })
    }
}

struct ResolveFrame {
    answer: Option<PithResult<Value>>,
}

impl PureRuleFrame for ResolveFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        match self.answer.take() {
            Some(Ok(value)) => Ok(PureStep::Complete(value)),
            Some(Err(diagnostics)) => Err(diagnostics),
            None => Err(crate::diag("the resolve step ran twice")),
        }
    }
}

/// Decode the request's five inputs, resolve the scheme name against the
/// registered orderings, and run the search.
fn solve_from_values(schemes: &Schemes, inputs: &[Value]) -> PithResult<Value> {
    let [scheme, constraints, universe, preferences, budget] = inputs else {
        return Err(crate::diag(format!(
            "a resolve request supplies five inputs; found {}",
            inputs.len()
        )));
    };
    let ordering = requested_scheme(schemes, scheme)?;
    let Value::List(entries) = constraints else {
        return Err(crate::diag(
            "the second resolve input is not a constraint set",
        ));
    };
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries.iter() {
        parsed.push(Constraint::from_value(entry)?);
    }
    let preferences = preference_list_from_value(preferences)?;
    let Value::Int(budget) = budget else {
        return Err(crate::diag(
            "the fifth resolve input is not a budget integer",
        ));
    };
    let budget = u64::try_from(*budget)
        .map_err(|_| crate::diag("the resolve budget must not be negative"))?;
    let request = SolveRequest {
        constraints: parsed.into(),
        universe: CandidateUniverse::from_value(universe)?,
        preferences,
        budget,
    };
    Ok(resolve(ordering, &request).to_value())
}

/// Re-exported so callers that build requests need one import for the whole
/// host-rule surface: the rule, the schemes it resolves names against, and
/// the request builder for the interface it serves.
pub use crate::resolution::{Resolution, resolve_request};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::version_scheme_value;

    #[test]
    fn a_scheme_this_registration_does_not_carry_is_named_in_the_diagnostic() {
        let solver = ResolveSolver::new(Schemes::standard());
        let result = solve_from_values(
            &solver.schemes,
            &[
                version_scheme_value("semver"),
                Value::List(Box::new([])),
                Value::List(Box::new([])),
                Value::List(Box::new([])),
                Value::Int(10),
            ],
        );
        let error = result.unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("semver") && d.message.0.contains(NUMERIC_SEGMENTS)),
            "the diagnostic names the requested scheme and the registered ones: {error:?}"
        );

        let duplicate = Schemes::new(Box::new([
            (Box::from("same"), Box::new(NumericSegments) as Scheme),
            (Box::from("same"), Box::new(Debian) as Scheme),
        ]))
        .err()
        .expect("two orderings cannot own one declared name");
        assert!(
            duplicate
                .iter()
                .any(|diagnostic| diagnostic.message.0.contains("same")
                    && diagnostic.message.0.contains("twice")),
            "the diagnostic names the duplicate semantic owner: {duplicate:?}"
        );
    }
}
