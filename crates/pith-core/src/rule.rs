//! Rules, typed interfaces, requests, and deterministic selection.

use pith_arena::define_arena;
use pith_diag::{Diag, EngineCode, Span};
use pith_ids::{PureComputationDigest, RuleIdentity, RuleRevision};
use smallvec::SmallVec;
use std::marker::PhantomData;

use crate::{
    EffectCategory, Pure, Type, Value,
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
        let mut computation_manifest = Vec::new();
        computation_manifest.extend_from_slice(rule.identity.digest().as_bytes());
        computation_manifest.extend_from_slice(rule.revision.digest().as_bytes());
        encode_interface(&mut computation_manifest, &request.interface);
        encode_length(&mut computation_manifest, request.inputs.len());
        request
            .inputs
            .iter()
            .for_each(|value| encode_value_payload(&mut computation_manifest, value));

        Self {
            rule_identity: rule.identity,
            rule_revision: rule.revision,
            digest: PureComputationDigest::of_manifest(&computation_manifest),
        }
    }
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
            let actual = value.value_type();
            if actual != *expected {
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
    use crate::value_codec::{
        TAG_BLOB, TAG_BOOL, TAG_BYTES, TAG_INT, TAG_NOMINAL, TAG_TEXT, TAG_UNIT,
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
            (Type::Nominal { name: "n".into() }, TAG_NOMINAL),
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

    #[test]
    fn nominal_types_participate_in_pure_computation_identity() {
        let first_interface = interface([], Type::Nominal { name: "A".into() });
        let second_interface = interface([], Type::Nominal { name: "B".into() });
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
