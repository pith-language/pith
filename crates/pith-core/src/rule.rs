//! Rules, typed interfaces, requests, and deterministic selection.

use indexmap::IndexMap;
use pith_arena::define_arena;
use pith_diag::{Diag, EngineCode, Span};
use pith_ids::{
    ActionComputationDigest, ActionSpecDigest, ObservationComputationDigest, PureComputationDigest,
    RuleIdentity, RuleRevision,
};
use smallvec::SmallVec;
use std::marker::PhantomData;

use crate::{
    Action, Coordinate, EffectCategory, Observation, Pure, Type, Value,
    manifest::encode_length,
    value_codec::{encode_type_payload, encode_value_payload},
};

define_arena!(RuleId, RuleArena, RuleBrand);

/// The typed signature a rule provides and a request requires.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Interface {
    pub inputs: Box<[Type]>,
    pub output: Type,
}

impl std::fmt::Display for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("(")?;
        let mut separator = "";
        for input in &self.inputs {
            f.write_str(separator)?;
            write!(f, "{input}")?;
            separator = ", ";
        }
        write!(f, ") -> {}", self.output)
    }
}

/// A request for a typed result. `label` exists only for diagnostics and
/// provenance; selection uses `interface` exclusively (decision 0015).
#[derive(Clone, Debug)]
pub struct Request<K: EffectCategory = Pure> {
    pub label: Box<str>,
    pub interface: Interface,
    pub inputs: Box<[Value]>,
    pub span: Span,
    effect: PhantomData<fn() -> K>,
}

#[derive(Clone, Debug)]
pub struct Rule<K: EffectCategory = Pure> {
    pub coordinate: Coordinate,
    pub tier: RuleTier,
    pub identity: RuleIdentity,
    pub revision: RuleRevision,
    pub label: Box<str>,
    pub interface: Interface,
    pub span: Span,
    effect: PhantomData<fn() -> K>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuleTier {
    Host,
}

/// Persistent identity for a pure rule application over the current semantic
/// IR subset.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PureComputationKey {
    pub rule_identity: RuleIdentity,
    pub rule_revision: RuleRevision,
    /// Digest of the rule identity, revision, request interface, and inputs.
    pub digest: PureComputationDigest,
}

impl PureComputationKey {
    pub fn new(rule: &Rule<Pure>, request: &Request<Pure>) -> Self {
        Self {
            rule_identity: rule.identity,
            rule_revision: rule.revision,
            digest: PureComputationDigest::of_manifest(&encode_application(rule, request)),
        }
    }
}

/// Persistent identity for an action rule application (decision 0031).
///
/// The request half of action identity: which rule, at which revision, was
/// applied to which inputs, and what contract it planned from them. The
/// execution half — resolved platform, installed confinement, produced content
/// — is knowable only after the action has run, and a key computable only
/// after running would have nothing to find. Decision 0031 tests those facts
/// against a recorded attempt when reuse is considered.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionComputationKey {
    pub rule_identity: RuleIdentity,
    pub rule_revision: RuleRevision,
    /// Digest of the rule identity, revision, request interface, inputs, and
    /// the digest of the contract the rule planned from those inputs.
    pub digest: ActionComputationDigest,
}

impl ActionComputationKey {
    /// `spec_digest` is the digest of the contract `rule` planned from
    /// `request`, which the caller has already computed to validate the plan.
    ///
    /// The key commits to it and to the request inputs. An action rule body
    /// plans a contract from its inputs and completes a result from those same
    /// inputs and the execution, so two requests that plan one contract can
    /// still complete to different results.
    pub fn new(
        rule: &Rule<Action>,
        request: &Request<Action>,
        spec_digest: ActionSpecDigest,
    ) -> Self {
        Self::from_parts(
            rule.identity,
            rule.revision,
            &request.interface,
            &request.inputs,
            spec_digest,
        )
    }

    /// The same key, derived from material a durable record holds rather than
    /// from a live [`Rule`] and [`Request`].
    ///
    /// Decision 0033 has an action computation retain the interface and inputs
    /// its key was built over, so a store can rebuild the key it filed the
    /// record under and check it instead of trusting the digest beside it.
    pub fn from_parts(
        rule_identity: RuleIdentity,
        rule_revision: RuleRevision,
        interface: &Interface,
        inputs: &[Value],
        spec_digest: ActionSpecDigest,
    ) -> Self {
        let mut manifest =
            encode_application_parts(rule_identity, rule_revision, interface, inputs);
        manifest.extend_from_slice(spec_digest.digest().as_bytes());
        Self {
            rule_identity,
            rule_revision,
            digest: ActionComputationDigest::of_manifest(&manifest),
        }
    }
}

/// Cache-invalidating identity of one observation rule application (decision
/// 0060), on the split 0031 fixed for actions: the request half is a key, and
/// the world half — the revision an observer attested — is tested when a
/// recorded attempt is considered for reuse.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObservationComputationKey {
    pub rule_identity: RuleIdentity,
    pub rule_revision: RuleRevision,
    /// Digest of the rule identity, revision, request interface, inputs, and
    /// the subject the rule derived from those inputs.
    pub digest: ObservationComputationDigest,
}

impl ObservationComputationKey {
    /// `subject` is the value naming what is observed, which the observation
    /// rule derived from `request`'s inputs the way an action rule plans a
    /// contract. The key commits to it and to the request inputs.
    pub fn new(rule: &Rule<Observation>, request: &Request<Observation>, subject: &Value) -> Self {
        Self::from_parts(
            rule.identity,
            rule.revision,
            &request.interface,
            &request.inputs,
            subject,
        )
    }

    /// The same key, derived from material a durable record holds rather than
    /// from a live [`Rule`] and [`Request`], on the same terms as
    /// [`ActionComputationKey::from_parts`].
    pub fn from_parts(
        rule_identity: RuleIdentity,
        rule_revision: RuleRevision,
        interface: &Interface,
        inputs: &[Value],
        subject: &Value,
    ) -> Self {
        let mut manifest =
            encode_application_parts(rule_identity, rule_revision, interface, inputs);
        manifest.extend_from_slice(&subject.encode_canonical());
        Self {
            rule_identity,
            rule_revision,
            digest: ObservationComputationDigest::of_manifest(&manifest),
        }
    }
}

/// The manifest prefix every computation key shares: which rule, at which
/// revision, was applied to which typed inputs. Each effect category appends
/// what it needs and hashes under its own domain prefix, so two keys over
/// identical material cannot collide.
fn encode_application<K: EffectCategory>(rule: &Rule<K>, request: &Request<K>) -> Vec<u8> {
    encode_application_parts(
        rule.identity,
        rule.revision,
        &request.interface,
        &request.inputs,
    )
}

fn encode_application_parts(
    rule_identity: RuleIdentity,
    rule_revision: RuleRevision,
    interface: &Interface,
    inputs: &[Value],
) -> Vec<u8> {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(rule_identity.digest().as_bytes());
    manifest.extend_from_slice(rule_revision.digest().as_bytes());
    encode_interface(&mut manifest, interface);
    encode_length(&mut manifest, inputs.len());
    inputs
        .iter()
        .for_each(|value| encode_value_payload(&mut manifest, value));
    manifest
}

impl<K: EffectCategory> Request<K> {
    pub fn new(
        label: impl Into<Box<str>>,
        interface: Interface,
        inputs: impl Into<Box<[Value]>>,
        span: Span,
    ) -> Self {
        Self {
            label: label.into(),
            interface,
            inputs: inputs.into(),
            span,
            effect: PhantomData,
        }
    }

    /// # Errors
    /// `E-1103` when an input is absent or has the wrong type.
    pub fn validate_inputs(&self) -> Result<(), Diag> {
        if self.inputs.len() != self.interface.inputs.len() {
            return Err(Diag::engine(
                EngineCode::RequestInputsMismatch,
                self.span,
                format!(
                    "request `{}` expects {} inputs but received {}",
                    self.label,
                    self.interface.inputs.len(),
                    self.inputs.len()
                ),
            ));
        }

        for (position, (value, expected)) in self
            .inputs
            .iter()
            .zip(self.interface.inputs.iter())
            .enumerate()
        {
            if !value.is_type(expected) {
                let actual = value.value_type();
                return Err(Diag::engine(
                    EngineCode::RequestInputsMismatch,
                    self.span,
                    format!(
                        "input {} for request `{}` has type {}, expected {}",
                        position.saturating_add(1),
                        self.label,
                        actual,
                        expected
                    ),
                ));
            }
        }
        Ok(())
    }
}

impl<K: EffectCategory> Rule<K> {
    pub fn new(
        module: impl Into<Box<str>>,
        revision: RuleRevision,
        label: impl Into<Box<str>>,
        interface: Interface,
        span: Span,
    ) -> Self {
        let label = label.into();
        Self {
            coordinate: Coordinate::new(module, label.clone()),
            tier: RuleTier::Host,
            identity: revision.rule_identity(),
            revision,
            label,
            interface,
            span,
            effect: PhantomData,
        }
    }

    /// Construct a host rule whose revision covers its body revision and interface.
    pub fn declared(
        module: &str,
        label: &str,
        body: BodyRevision,
        interface: Interface,
        span: Span,
    ) -> Self {
        let coordinate = Coordinate::new(module, label);
        let identity = RuleIdentity::of_module_declaration(module, label);
        let revision = RuleRevision::of_manifest(identity, &revision_manifest(body, &interface));
        Self {
            coordinate,
            tier: RuleTier::Host,
            identity,
            revision,
            label: label.into(),
            interface,
            span,
            effect: PhantomData,
        }
    }
}

/// The author-maintained revision of a host rule body.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyRevision(pub u32);

fn revision_manifest(body: BodyRevision, interface: &Interface) -> Vec<u8> {
    let mut manifest = body.0.to_le_bytes().to_vec();
    encode_interface(&mut manifest, interface);
    manifest
}

fn encode_interface(manifest: &mut Vec<u8>, interface: &Interface) {
    encode_length(manifest, interface.inputs.len());
    interface
        .inputs
        .iter()
        .for_each(|input| encode_type_payload(manifest, input));
    encode_type_payload(manifest, &interface.output);
}

/// Zero, one, or multiple matching providers. Ambiguity is never ranked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectOutcome {
    NoMatch,
    One(RuleId),
    Ambiguous(SmallVec<[RuleId; 2]>),
}

/// The registered rules of one effect category, indexed by the interface they
/// provide (decision 0057).
///
/// The arena is the population and the index is a view of it, so the table owns
/// both and is the only way to add a rule. There is no accessor handing out a
/// mutable rule: an interface that could be edited in place would leave the
/// index naming a bucket the rule no longer belongs to, and registration is the
/// one event either structure has to observe.
pub struct RuleTable<K: EffectCategory = Pure> {
    rules: RuleArena<Rule<K>>,
    by_interface: IndexMap<Interface, SmallVec<[RuleId; 2]>>,
}

impl<K: EffectCategory> RuleTable<K> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rules: RuleArena::new(),
            by_interface: IndexMap::new(),
        }
    }

    /// Register `rule` and return its id.
    pub fn push(&mut self, rule: Rule<K>) -> RuleId {
        let interface = rule.interface.clone();
        let id = self.rules.push(rule);
        self.by_interface.entry(interface).or_default().push(id);
        id
    }

    #[must_use]
    pub fn get(&self, id: RuleId) -> Option<&Rule<K>> {
        self.rules.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (RuleId, &Rule<K>)> {
        self.rules.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Select the rules providing the request's interface, independent of
    /// registration order (decision 0015).
    ///
    /// The index keys on the interface under the same `Eq` the scan this
    /// replaced evaluated per rule, so a bucket holds exactly the rules that
    /// scan would have kept. Ordering the candidates is left to the ambiguous
    /// branch, which is a failure path: a run that reports `E-1102` is about to
    /// stop, and the ordinary outcome allocates nothing.
    #[must_use]
    pub fn select(&self, request: &Request<K>) -> SelectOutcome {
        let Some(candidates) = self.by_interface.get(&request.interface) else {
            return SelectOutcome::NoMatch;
        };
        match candidates.as_slice() {
            [] => SelectOutcome::NoMatch,
            [only] => SelectOutcome::One(*only),
            _ => {
                let mut candidates = candidates.clone();
                candidates.sort_by(|left, right| self.label(*left).cmp(&self.label(*right)));
                SelectOutcome::Ambiguous(candidates)
            }
        }
    }

    fn label(&self, id: RuleId) -> Option<&str> {
        self.rules.get(id).map(|rule| rule.label.as_ref())
    }
}

impl<K: EffectCategory> Default for RuleTable<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectOutcome {
    /// # Errors
    /// `E-1101` when no rule matched; `E-1102` naming every candidate when more
    /// than one matched.
    pub fn into_result<K: EffectCategory>(
        self,
        request: &Request<K>,
        rules: &RuleTable<K>,
    ) -> Result<RuleId, Diag> {
        match self {
            Self::One(id) => Ok(id),
            Self::NoMatch => Err(Diag::engine(
                EngineCode::NoRuleForInterface,
                request.span,
                format!(
                    "no rule satisfies `{}` ({})",
                    request.label, request.interface
                ),
            )),
            Self::Ambiguous(candidates) => {
                let mut diag = Diag::engine(
                    EngineCode::AmbiguousRule,
                    request.span,
                    format!(
                        "ambiguous rule for `{}` ({})",
                        request.label, request.interface
                    ),
                );
                for id in candidates {
                    if let Some(rule) = rules.get(id) {
                        diag = diag.with_note(
                            rule.span,
                            format!("candidate `{}`: {}", rule.label, rule.interface),
                        );
                    }
                }
                Err(diag)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{declared_nominal, declared_sum};
    use crate::value_codec::{
        TAG_BLOB, TAG_BOOL, TAG_BYTES, TAG_INT, TAG_LIST, TAG_NOMINAL, TAG_RECORD, TAG_SUM,
        TAG_TEXT, TAG_UNIT,
    };

    #[test]
    fn type_and_value_tags_are_distinct_and_share_numbering() {
        // The pure-computation manifest format is provisional, so this does not
        // pin exact tag numbers. It pins the two invariants that matter: every
        // variant encodes under a distinct tag (no collision), and each Value
        // variant shares its tag with the matching Type variant.
        let types = [
            (Type::Unit, TAG_UNIT),
            (Type::Bool, TAG_BOOL),
            (Type::Int, TAG_INT),
            (Type::Text, TAG_TEXT),
            (Type::Bytes, TAG_BYTES),
            (Type::Blob, TAG_BLOB),
            (declared_nominal("test", "n", Type::Blob), TAG_NOMINAL),
            (Type::List(Box::new(Type::Unit)), TAG_LIST),
            (
                Type::record([crate::RecordField {
                    name: "n".into(),
                    payload: Type::Unit,
                }])
                .unwrap(),
                TAG_RECORD,
            ),
            (
                declared_sum(
                    "test",
                    "s",
                    [crate::SumConstructor {
                        name: "c".into(),
                        payload: None,
                    }],
                ),
                TAG_SUM,
            ),
        ];
        for (i, (_, tag_i)) in types.iter().enumerate() {
            for (_, tag_j) in types.iter().skip(i + 1) {
                assert_ne!(tag_i, tag_j, "two Type variants share a manifest tag");
            }
        }
        let values = [
            (Value::Unit, TAG_UNIT),
            (Value::Bool(false), TAG_BOOL),
            (Value::int(0), TAG_INT),
            (Value::Text("".into()), TAG_TEXT),
            (Value::Bytes(Box::new([])), TAG_BYTES),
            (
                Value::Blob(pith_ids::ContentId::from_digest(
                    pith_ids::ContentDigest::from_bytes([0; pith_ids::DIGEST_LEN]),
                )),
                TAG_BLOB,
            ),
            (Value::List(Box::new([])), TAG_LIST),
            (
                Value::record([crate::RecordField {
                    name: "n".into(),
                    payload: Value::Unit,
                }])
                .unwrap(),
                TAG_RECORD,
            ),
            (
                Value::Sum {
                    type_name: "test.s".into(),
                    constructor: "c".into(),
                    payload: None,
                },
                TAG_SUM,
            ),
        ];
        for (value, expected) in &values {
            let mut manifest = Vec::new();
            encode_value_payload(&mut manifest, value);
            assert_eq!(
                manifest.first(),
                Some(expected),
                "a Value variant did not encode under its Type-shared tag"
            );
        }
    }

    fn interface(inputs: impl Into<Box<[Type]>>, output: Type) -> Interface {
        Interface {
            inputs: inputs.into(),
            output,
        }
    }

    fn rule(label: &str, interface: Interface) -> Rule {
        let identity = RuleIdentity::of_module_declaration("pith-core.rule-tests", label);
        let revision = RuleRevision::of_manifest(identity, b"rule-tests-provider-v1");
        Rule::new(
            "pith-core.rule-tests",
            revision,
            label,
            interface,
            Span::none(),
        )
    }

    fn request(label: &str, interface: Interface, inputs: impl Into<Box<[Value]>>) -> Request {
        Request::new(label, interface, inputs, Span::none())
    }

    fn pure_key(rule: &Rule, request: &Request) -> PureComputationKey {
        PureComputationKey::new(rule, request)
    }

    #[test]
    fn pure_computation_key_is_stable_for_same_application() {
        let signature = interface([Type::Int], Type::Text);
        let identity = RuleIdentity::of_module_declaration("example.module", "provider");
        let revision = RuleRevision::of_manifest(identity, b"provider-v1");
        let first_rule = Rule::new(
            "example.module",
            revision,
            "first diagnostic label",
            signature.clone(),
            Span::none(),
        );
        let second_rule = Rule::new(
            "example.module",
            revision,
            "second diagnostic label",
            signature.clone(),
            Span::none(),
        );
        let first = request("first request", signature.clone(), [Value::int(7)]);
        let second = request("second request", signature, [Value::int(7)]);

        assert_eq!(
            pure_key(&first_rule, &first),
            pure_key(&second_rule, &second)
        );
    }

    #[test]
    fn pure_computation_key_distinguishes_input_values() {
        let signature = interface([Type::Int], Type::Int);
        let selected = rule("identity", signature.clone());
        let seven = request("value", signature.clone(), [Value::int(7)]);
        let eight = request("value", signature, [Value::int(8)]);

        assert_ne!(pure_key(&selected, &seven), pure_key(&selected, &eight));
    }

    #[test]
    fn pure_computation_key_distinguishes_selected_rules() {
        let signature = interface([], Type::Int);
        let first = rule("first provider", signature.clone());
        let second = rule("second provider", signature.clone());
        let requested = request("value", signature, []);

        let first_key = pure_key(&first, &requested);
        let second_key = pure_key(&second, &requested);
        assert_ne!(first_key.rule_identity, second_key.rule_identity);
        assert_ne!(first_key, second_key);
    }

    #[test]
    fn pure_computation_key_distinguishes_rule_revisions() {
        let signature = interface([], Type::Int);
        let identity = RuleIdentity::of_module_declaration("example.module", "provider");
        let first = Rule::new(
            "example.module",
            RuleRevision::of_manifest(identity, b"provider-v1"),
            "provider",
            signature.clone(),
            Span::none(),
        );
        let second = Rule::new(
            "example.module",
            RuleRevision::of_manifest(identity, b"provider-v2"),
            "provider",
            signature.clone(),
            Span::none(),
        );
        let requested = request("value", signature, []);

        let first_key = pure_key(&first, &requested);
        let second_key = pure_key(&second, &requested);
        assert_eq!(first_key.rule_identity, second_key.rule_identity);
        assert_ne!(first_key.rule_revision, second_key.rule_revision);
        assert_ne!(first_key, second_key);
    }

    fn action_rule(label: &str, interface: Interface) -> Rule<Action> {
        let identity = RuleIdentity::of_module_declaration("pith-core.rule-tests", label);
        let revision = RuleRevision::of_manifest(identity, b"rule-tests-provider-v1");
        Rule::new(
            "pith-core.rule-tests",
            revision,
            label,
            interface,
            Span::none(),
        )
    }

    fn action_request(
        label: &str,
        interface: Interface,
        inputs: impl Into<Box<[Value]>>,
    ) -> Request<Action> {
        Request::new(label, interface, inputs, Span::none())
    }

    fn planned(contract: &[u8]) -> ActionSpecDigest {
        ActionSpecDigest::of_manifest(contract)
    }

    fn observation_rule(label: &str, interface: Interface) -> Rule<Observation> {
        let identity = RuleIdentity::of_module_declaration("pith-core.rule-tests", label);
        let revision = RuleRevision::of_manifest(identity, b"rule-tests-observation-v1");
        Rule::new(
            "pith-core.rule-tests",
            revision,
            label,
            interface,
            Span::none(),
        )
    }

    #[test]
    fn action_computation_key_is_stable_for_same_application() {
        let signature = interface([Type::Int], Type::Text);
        let identity = RuleIdentity::of_module_declaration("example.module", "compile");
        let revision = RuleRevision::of_manifest(identity, b"compile-v1");
        let first_rule = Rule::new(
            "example.module",
            revision,
            "first diagnostic label",
            signature.clone(),
            Span::none(),
        );
        let second_rule = Rule::new(
            "example.module",
            revision,
            "second diagnostic label",
            signature.clone(),
            Span::none(),
        );
        let first = action_request("first request", signature.clone(), [Value::int(7)]);
        let second = action_request("second request", signature, [Value::int(7)]);

        assert_eq!(
            ActionComputationKey::new(&first_rule, &first, planned(b"contract")),
            ActionComputationKey::new(&second_rule, &second, planned(b"contract"))
        );
    }

    #[test]
    fn action_computation_key_distinguishes_planned_contracts() {
        // The contract is what runs, so equal inputs planning different
        // contracts are different computations.
        let signature = interface([Type::Int], Type::Blob);
        let selected = action_rule("compile", signature.clone());
        let requested = action_request("object", signature, [Value::int(7)]);

        assert_ne!(
            ActionComputationKey::new(&selected, &requested, planned(b"gcc contract")),
            ActionComputationKey::new(&selected, &requested, planned(b"clang contract"))
        );
    }

    #[test]
    fn action_computation_key_distinguishes_request_inputs_at_one_contract() {
        // One contract reached from different inputs is still two
        // computations: the rule body completes its result from the inputs as
        // well as from the execution.
        let signature = interface([Type::Int], Type::Blob);
        let selected = action_rule("compile", signature.clone());
        let seven = action_request("object", signature.clone(), [Value::int(7)]);
        let eight = action_request("object", signature, [Value::int(8)]);

        assert_ne!(
            ActionComputationKey::new(&selected, &seven, planned(b"contract")),
            ActionComputationKey::new(&selected, &eight, planned(b"contract"))
        );
    }

    #[test]
    fn action_and_pure_keys_do_not_collide_over_the_same_application() {
        // Both keys hash the same rule identity, revision, interface, and
        // inputs, separated only by the domain prefix. A collision would let an
        // action attempt answer a pure request.
        let signature = interface([Type::Int], Type::Text);
        let identity = RuleIdentity::of_module_declaration("example.module", "provider");
        let revision = RuleRevision::of_manifest(identity, b"provider-v1");
        let pure_rule: Rule<Pure> = Rule::new(
            "example.module",
            revision,
            "provider",
            signature.clone(),
            Span::none(),
        );
        let action_rule: Rule<Action> = Rule::new(
            "example.module",
            revision,
            "provider",
            signature.clone(),
            Span::none(),
        );
        let pure_request = request("value", signature.clone(), [Value::int(7)]);
        let action_request = action_request("value", signature, [Value::int(7)]);

        let pure = PureComputationKey::new(&pure_rule, &pure_request);
        let action = ActionComputationKey::new(&action_rule, &action_request, planned(b"contract"));

        assert_eq!(pure.rule_identity, action.rule_identity);
        assert_ne!(pure.digest.digest(), action.digest.digest());
    }

    #[test]
    fn observation_computation_key_commits_to_inputs_and_derived_subject() {
        let signature = interface([Type::Text], Type::Text);
        let selected = observation_rule("file-mtime", signature.clone());
        let request = Request::<Observation>::new(
            "mtime",
            signature,
            [Value::Text("src/main.rs".into())],
            Span::none(),
        );
        let absolute = Value::Text("/workspace/src/main.rs".into());
        let other = Value::Text("/other/src/main.rs".into());

        assert_eq!(
            ObservationComputationKey::new(&selected, &request, &absolute),
            ObservationComputationKey::new(&selected, &request, &absolute),
        );
        assert_ne!(
            ObservationComputationKey::new(&selected, &request, &absolute),
            ObservationComputationKey::new(&selected, &request, &other),
        );
    }

    #[test]
    fn nominal_types_participate_in_pure_computation_identity() {
        let first_interface = interface([], declared_nominal("test", "A", Type::Blob));
        let second_interface = interface([], declared_nominal("test", "B", Type::Blob));
        let selected = rule("provider", first_interface.clone());
        let first = request("value", first_interface, []);
        let second = request("value", second_interface, []);

        assert_ne!(pure_key(&selected, &first), pure_key(&selected, &second));
    }

    #[test]
    fn blob_content_identity_participates_in_pure_computation_identity() {
        let signature = interface([Type::Blob], Type::Int);
        let selected = rule("blob length", signature.clone());
        let first = request(
            "value",
            signature.clone(),
            [Value::Blob(pith_ids::ContentId::of_blob(b"first"))],
        );
        let same = request(
            "other label",
            signature.clone(),
            [Value::Blob(pith_ids::ContentId::of_blob(b"first"))],
        );
        let second = request(
            "value",
            signature,
            [Value::Blob(pith_ids::ContentId::of_blob(b"second"))],
        );

        assert_eq!(pure_key(&selected, &first), pure_key(&selected, &same));
        assert_ne!(pure_key(&selected, &first), pure_key(&selected, &second));
    }

    #[test]
    fn no_match_produces_error_with_typed_signature() {
        let rules: RuleTable = RuleTable::new();
        let request = request("answer", interface([], Type::Int), []);
        let err = rules
            .select(&request)
            .into_result(&request, &rules)
            .unwrap_err();
        assert_eq!(err.code, EngineCode::NoRuleForInterface.into());
        assert_eq!(err.severity, pith_diag::Severity::Error);
        assert!(err.message.0.contains("() -> Int"));
    }

    #[test]
    fn labels_do_not_participate_in_selection() {
        let mut rules: RuleTable = RuleTable::new();
        let bool_rule = rules.push(rule("same label", interface([], Type::Bool)));
        let int_rule = rules.push(rule("same label", interface([], Type::Int)));
        let request = request("different label", interface([], Type::Int), []);

        let selected = rules
            .select(&request)
            .into_result(&request, &rules)
            .unwrap();

        assert_ne!(selected, bool_rule);
        assert_eq!(selected, int_rule);
    }

    #[test]
    fn ambiguity_names_every_candidate_and_interface() {
        let mut rules: RuleTable = RuleTable::new();
        let signature = interface([Type::Text], Type::Int);
        rules.push(rule("a", signature.clone()));
        rules.push(rule("b", signature.clone()));
        let request = request("thing", signature, [Value::Text("input".into())]);

        let err = rules
            .select(&request)
            .into_result(&request, &rules)
            .unwrap_err();

        assert_eq!(err.code, EngineCode::AmbiguousRule.into());
        assert_eq!(err.notes.len(), 2);
        assert!(
            err.notes
                .iter()
                .all(|note| note.message.0.contains("(Text) -> Int"))
        );
    }

    #[test]
    fn ambiguity_diagnostics_ignore_registration_order() {
        fn notes(reversed: bool) -> Vec<Box<str>> {
            let mut rules: RuleTable = RuleTable::new();
            let signature = interface([], Type::Int);
            let labels = if reversed {
                ["beta", "alpha"]
            } else {
                ["alpha", "beta"]
            };
            for label in labels {
                rules.push(rule(label, signature.clone()));
            }
            let request = request("answer", signature, []);
            rules
                .select(&request)
                .into_result(&request, &rules)
                .unwrap_err()
                .notes
                .iter()
                .map(|note| note.message.0.clone())
                .collect()
        }

        assert_eq!(notes(false), notes(true));
    }

    #[test]
    fn request_inputs_must_match_the_declared_interface() {
        let request = Request::<Pure>::new(
            "answer",
            interface([Type::Int], Type::Int),
            [Value::Bool(true)],
            Span::none(),
        );

        let err = request.validate_inputs().unwrap_err();

        assert_eq!(err.code, EngineCode::RequestInputsMismatch.into());
        assert!(err.message.0.contains("has type Bool, expected Int"));
    }
}

#[cfg(test)]
mod derived_revisions {
    use super::*;
    use crate::declaration::DeclarationTable;
    use crate::value::declared_nominal;

    fn interface_over(input: Type, output: Type) -> Interface {
        Interface {
            inputs: [input].into(),
            output,
        }
    }

    fn revision_of(interface: Interface) -> RuleRevision {
        Rule::<Pure>::declared("m", "r", BodyRevision(1), interface, Span::none()).revision
    }

    #[test]
    fn a_changed_representation_moves_the_revision_with_no_author_edit() {
        // The reason decision 0047's last section exists. Before it, a rule's
        // revision was a hand-written constant, so editing a nominal type's
        // representation moved nothing and a cached result computed under the old
        // one was still served.
        let over_blob = revision_of(interface_over(
            declared_nominal("m", "CSource", Type::Blob),
            Type::Unit,
        ));
        let over_text = revision_of(interface_over(
            declared_nominal("m", "CSource", Type::Text),
            Type::Unit,
        ));
        assert_ne!(over_blob, over_text);
    }

    #[test]
    fn an_unrelated_declaration_does_not_move_a_revision() {
        // The other half of the granularity claim: the old constant moved every
        // rule in a library at once, and a derived revision moves only the rules
        // whose interface reaches what changed.
        let mut table = DeclarationTable::new("m");
        let source = table.nominal("CSource", Type::Blob).unwrap();
        let before = revision_of(interface_over(source.clone(), Type::Unit));

        // Declaring something else, and changing what it is, is invisible here.
        let mut other = DeclarationTable::new("m");
        other.nominal("CSource", Type::Blob).unwrap();
        other.nominal("Unrelated", Type::Text).unwrap();
        let after = revision_of(interface_over(
            Type::of_declaration(other.get("CSource").unwrap()),
            Type::Unit,
        ));
        assert_eq!(before, after);
    }

    #[test]
    fn a_changed_interface_shape_moves_the_revision() {
        let source = declared_nominal("m", "CSource", Type::Blob);
        assert_ne!(
            revision_of(interface_over(source.clone(), Type::Unit)),
            revision_of(interface_over(source.clone(), Type::Bool)),
        );
        assert_ne!(
            revision_of(interface_over(source.clone(), Type::Unit)),
            revision_of(Interface {
                inputs: [source, Type::Bool].into(),
                output: Type::Unit,
            }),
        );
    }

    #[test]
    fn a_declared_rules_identity_is_its_coordinate_and_survives_a_revision_move() {
        // 0023's two halves, at the rule layer: the coordinate is stable across a
        // representation change, the revision is not.
        let over_blob = Rule::<Pure>::declared(
            "m",
            "compile",
            BodyRevision(1),
            interface_over(declared_nominal("m", "CSource", Type::Blob), Type::Unit),
            Span::none(),
        );
        let over_text = Rule::<Pure>::declared(
            "m",
            "compile",
            BodyRevision(1),
            interface_over(declared_nominal("m", "CSource", Type::Text), Type::Unit),
            Span::none(),
        );
        assert_eq!(over_blob.identity, over_text.identity);
        assert_ne!(over_blob.revision, over_text.revision);
        assert_eq!(
            over_blob.identity,
            RuleIdentity::of_module_declaration("m", "compile")
        );
    }

    #[test]
    fn a_body_revision_moves_the_revision_and_nothing_else() {
        // The half the interface cannot see (decision 0023). Without it a host
        // rule whose body changes while its interface holds still has no way to
        // invalidate, which is what deleting the per-library constants would have
        // cost if the derivation had been the only input.
        let interface = interface_over(declared_nominal("m", "CSource", Type::Blob), Type::Unit);
        let at = |body: u32| {
            Rule::<Pure>::declared(
                "m",
                "r",
                BodyRevision(body),
                interface.clone(),
                Span::none(),
            )
        };
        assert_ne!(at(1).revision, at(2).revision);
        assert_eq!(at(1).revision, at(1).revision);
        // The coordinate is not the revision: bumping a body leaves identity put.
        assert_eq!(at(1).identity, at(2).identity);
    }

    #[test]
    fn the_two_revision_halves_are_independent() {
        // Neither half can mask the other: a body bump is visible at every
        // interface, and an interface change is visible at every body revision.
        let blob = interface_over(declared_nominal("m", "CSource", Type::Blob), Type::Unit);
        let text = interface_over(declared_nominal("m", "CSource", Type::Text), Type::Unit);
        let at = |body: u32, interface: Interface| {
            Rule::<Pure>::declared("m", "r", BodyRevision(body), interface, Span::none()).revision
        };
        let (a, b, c, d) = (
            at(1, blob.clone()),
            at(2, blob),
            at(1, text.clone()),
            at(2, text),
        );
        for (left, right) in [(&a, &b), (&a, &c), (&a, &d), (&b, &c), (&b, &d), (&c, &d)] {
            assert_ne!(left, right, "two of the four combinations collided");
        }
    }

    #[test]
    fn a_body_revision_does_not_leak_across_rules() {
        // Per rule, not per library: the defect the retired constants had in the
        // other direction, where one edit moved every rule beside it.
        let interface = interface_over(Type::Unit, Type::Unit);
        let bumped = Rule::<Pure>::declared(
            "m",
            "compile",
            BodyRevision(2),
            interface.clone(),
            Span::none(),
        );
        let untouched = Rule::<Pure>::declared(
            "m",
            "link",
            BodyRevision(1),
            interface.clone(),
            Span::none(),
        );
        let untouched_again =
            Rule::<Pure>::declared("m", "link", BodyRevision(1), interface, Span::none());
        assert_eq!(untouched.revision, untouched_again.revision);
        assert_ne!(bumped.revision, untouched.revision);
    }

    #[test]
    fn two_modules_declaring_one_rule_label_are_two_rules() {
        assert_ne!(
            Rule::<Pure>::declared(
                "xylem",
                "compile",
                BodyRevision(1),
                interface_over(Type::Unit, Type::Unit),
                Span::none()
            )
            .identity,
            Rule::<Pure>::declared(
                "phloem",
                "compile",
                BodyRevision(1),
                interface_over(Type::Unit, Type::Unit),
                Span::none()
            )
            .identity,
        );
    }
}
