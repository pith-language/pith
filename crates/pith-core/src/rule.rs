//! Rules, typed interfaces, requests, and deterministic selection.

use pith_arena::define_arena;
use pith_diag::{Diag, EngineCode, Span};
use pith_ids::{
    ActionComputationDigest, ActionSpecDigest, PureComputationDigest, RuleIdentity, RuleRevision,
};
use smallvec::SmallVec;
use std::marker::PhantomData;

use crate::{
    Action, EffectCategory, Pure, Type, Value,
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
    pub identity: RuleIdentity,
    pub revision: RuleRevision,
    pub label: Box<str>,
    pub interface: Interface,
    pub span: Span,
    effect: PhantomData<fn() -> K>,
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
        revision: RuleRevision,
        label: impl Into<Box<str>>,
        interface: Interface,
        span: Span,
    ) -> Self {
        Self {
            identity: revision.rule_identity(),
            revision,
            label: label.into(),
            interface,
            span,
            effect: PhantomData,
        }
    }

    /// A rule declared at `module`.`label`, with its revision derived from the
    /// declarations `interface` names (decision 0047).
    ///
    /// This is the constructor a library should use. It replaces a hand-bumped
    /// revision manifest — the "manual semantic version only" alternative 0023
    /// rejected and every domain library nonetheless shipped — with a derivation
    /// over the interface. Changing a nominal type's representation moves the
    /// revision of every rule whose interface names it, with no author edit;
    /// changing an unrelated declaration moves nothing.
    ///
    /// The interface cannot see a host-tier rule *body* that changes while it and
    /// every declaration in it stay fixed, which is why `body` is the second
    /// input and not a convenience: decision 0023 asks for both halves, and says
    /// an implementation-local version "can be one revision-manifest input, but
    /// it is not the only input for rust-hosted code."
    ///
    /// So the two halves cover the two ways a rule's meaning moves. An interface
    /// or declaration change is automatic. A body change is an explicit bump of
    /// `body`, per rule — which is what the retired per-library constants got
    /// wrong in the other direction: one edit moved every rule in the library.
    /// The bump stops being the author's job when 0038's represented tier lands
    /// and a body's revision derives from canonical ir.
    pub fn declared(
        module: &str,
        label: &str,
        body: BodyRevision,
        interface: Interface,
        span: Span,
    ) -> Self {
        let identity = RuleIdentity::of_module_declaration(module, label);
        let revision = RuleRevision::of_manifest(identity, &revision_manifest(body, &interface));
        Self {
            identity,
            revision,
            label: label.into(),
            interface,
            span,
            effect: PhantomData,
        }
    }
}

/// The author-maintained half of a rule's revision: a number the author bumps
/// when the rule's body changes in a way that could change a result
/// (decision 0023).
///
/// Per rule rather than per library. The declaration half of the revision is
/// derived and needs no bump, so this moves only for what the derivation cannot
/// see, and moving it invalidates one rule instead of every rule beside it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyRevision(pub u32);

/// The revision manifest for a rule declared over `interface` at `body`.
///
/// Two inputs: the author's body revision, and the canonical interface encoding.
///
/// Decision 0047 specifies this as the interface encoding *plus* the sorted
/// digests of every declaration that encoding reaches. The second half is
/// redundant under the first half of the same record: because a nominal or sum
/// type carries its declaration rather than a bare coordinate, the encoding
/// already contains every declaration body the interface reaches, so no
/// declaration change can move a digest without moving the encoding, and no
/// encoding is reachable from two different declaration sets. Adding the digests
/// would be a second mechanism for one concern, which the principles forbid.
///
/// If a later record moves to ambient table resolution — the alternative 0047
/// considered and rejected, where a type carries a bare coordinate — the digest
/// half becomes load-bearing and this manifest has to grow it. Under 0048 that is
/// a digest-domain bump rather than a silent basis change.
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

/// Select rules by exact typed-interface match, independent of registration
/// order. Candidate order is canonical so diagnostics are deterministic.
#[must_use]
pub fn select_rule<K: EffectCategory>(
    request: &Request<K>,
    rules: &RuleArena<Rule<K>>,
) -> SelectOutcome {
    let mut candidates: Vec<(Interface, Box<str>, RuleId)> = rules
        .iter()
        .filter(|(_, rule)| rule.interface == request.interface)
        .map(|(id, rule)| (rule.interface.clone(), rule.label.clone(), id))
        .collect();
    candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    let candidates: SmallVec<[RuleId; 2]> = candidates.into_iter().map(|(_, _, id)| id).collect();
    match candidates.as_slice() {
        [] => SelectOutcome::NoMatch,
        [only] => SelectOutcome::One(*only),
        _ => SelectOutcome::Ambiguous(candidates),
    }
}

impl SelectOutcome {
    /// # Errors
    /// `E-1101` when no rule matched; `E-1102` naming every candidate when more
    /// than one matched.
    pub fn into_result<K: EffectCategory>(
        self,
        request: &Request<K>,
        rules: &RuleArena<Rule<K>>,
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
    use pith_arena::Arena;

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
            (Value::Int(0), TAG_INT),
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
        Rule::new(revision, label, interface, Span::none())
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
            revision,
            "first diagnostic label",
            signature.clone(),
            Span::none(),
        );
        let second_rule = Rule::new(
            revision,
            "second diagnostic label",
            signature.clone(),
            Span::none(),
        );
        let first = request("first request", signature.clone(), [Value::Int(7)]);
        let second = request("second request", signature, [Value::Int(7)]);

        assert_eq!(
            pure_key(&first_rule, &first),
            pure_key(&second_rule, &second)
        );
    }

    #[test]
    fn pure_computation_key_distinguishes_input_values() {
        let signature = interface([Type::Int], Type::Int);
        let selected = rule("identity", signature.clone());
        let seven = request("value", signature.clone(), [Value::Int(7)]);
        let eight = request("value", signature, [Value::Int(8)]);

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
            RuleRevision::of_manifest(identity, b"provider-v1"),
            "provider",
            signature.clone(),
            Span::none(),
        );
        let second = Rule::new(
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
        Rule::new(revision, label, interface, Span::none())
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

    #[test]
    fn action_computation_key_is_stable_for_same_application() {
        let signature = interface([Type::Int], Type::Text);
        let identity = RuleIdentity::of_module_declaration("example.module", "compile");
        let revision = RuleRevision::of_manifest(identity, b"compile-v1");
        let first_rule = Rule::new(
            revision,
            "first diagnostic label",
            signature.clone(),
            Span::none(),
        );
        let second_rule = Rule::new(
            revision,
            "second diagnostic label",
            signature.clone(),
            Span::none(),
        );
        let first = action_request("first request", signature.clone(), [Value::Int(7)]);
        let second = action_request("second request", signature, [Value::Int(7)]);

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
        let requested = action_request("object", signature, [Value::Int(7)]);

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
        let seven = action_request("object", signature.clone(), [Value::Int(7)]);
        let eight = action_request("object", signature, [Value::Int(8)]);

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
        let pure_rule: Rule<Pure> =
            Rule::new(revision, "provider", signature.clone(), Span::none());
        let action_rule: Rule<Action> =
            Rule::new(revision, "provider", signature.clone(), Span::none());
        let pure_request = request("value", signature.clone(), [Value::Int(7)]);
        let action_request = action_request("value", signature, [Value::Int(7)]);

        let pure = PureComputationKey::new(&pure_rule, &pure_request);
        let action = ActionComputationKey::new(&action_rule, &action_request, planned(b"contract"));

        assert_eq!(pure.rule_identity, action.rule_identity);
        assert_ne!(pure.digest.digest(), action.digest.digest());
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
        let arena: RuleArena<Rule> = Arena::new();
        let request = request("answer", interface([], Type::Int), []);
        let err = select_rule(&request, &arena)
            .into_result(&request, &arena)
            .unwrap_err();
        assert_eq!(err.code, EngineCode::NoRuleForInterface.into());
        assert_eq!(err.severity, pith_diag::Severity::Error);
        assert!(err.message.0.contains("() -> Int"));
    }

    #[test]
    fn labels_do_not_participate_in_selection() {
        let mut arena: RuleArena<Rule> = Arena::new();
        let bool_rule = arena.push(rule("same label", interface([], Type::Bool)));
        let int_rule = arena.push(rule("same label", interface([], Type::Int)));
        let request = request("different label", interface([], Type::Int), []);

        let selected = select_rule(&request, &arena)
            .into_result(&request, &arena)
            .unwrap();

        assert_ne!(selected, bool_rule);
        assert_eq!(selected, int_rule);
    }

    #[test]
    fn ambiguity_names_every_candidate_and_interface() {
        let mut arena: RuleArena<Rule> = Arena::new();
        let signature = interface([Type::Text], Type::Int);
        arena.push(rule("a", signature.clone()));
        arena.push(rule("b", signature.clone()));
        let request = request("thing", signature, [Value::Text("input".into())]);

        let err = select_rule(&request, &arena)
            .into_result(&request, &arena)
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
            let mut arena: RuleArena<Rule> = Arena::new();
            let signature = interface([], Type::Int);
            let labels = if reversed {
                ["beta", "alpha"]
            } else {
                ["alpha", "beta"]
            };
            for label in labels {
                arena.push(rule(label, signature.clone()));
            }
            let request = request("answer", signature, []);
            select_rule(&request, &arena)
                .into_result(&request, &arena)
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
