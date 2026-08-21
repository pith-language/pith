//! The pure key or action contract an attempt belongs to.

use pith_core::{ActionSpec, Interface, PureComputationKey, RuleRevision};
use pith_engine::ActionAuthorization;
use pith_engine::state::{
    DurableActionPlan, DurableActionRequest, DurableComputation, DurableObservationRequest,
    DurableRule, EncodedValue,
};
use pith_ids::{ActionComputationDigest, ObservationComputationDigest};

use crate::columns::{
    StoredActionDigest, StoredActionSpecDigest, StoredObservationDigest, StoredPureDigest,
    StoredRevisionDigest, StoredRuleIdentity,
};
use crate::schema::{action_request_inputs, computations, observation_request_inputs};

use diesel::prelude::*;
use diesel::sqlite::Sqlite;

use super::{Failure, corrupt};

#[derive(Queryable, Selectable)]
#[diesel(table_name = computations)]
#[diesel(check_for_backend(Sqlite))]
struct ComputationRow {
    id: i64,
    rule_identity: StoredRuleIdentity,
    rule_revision: StoredRevisionDigest,
    pure_digest: Option<StoredPureDigest>,
    action_digest: Option<StoredActionDigest>,
    observation_digest: Option<StoredObservationDigest>,
    action_spec_digest: Option<StoredActionSpecDigest>,
    action_spec: Option<Vec<u8>>,
    action_interface: Option<Vec<u8>>,
    observation_interface: Option<Vec<u8>>,
    observation_subject: Option<Vec<u8>>,
    observation_observer: Option<String>,
    authorization_denied: Option<bool>,
    authorization_policy: Option<String>,
    authorization_reason: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = computations)]
struct NewComputation {
    rule_identity: StoredRuleIdentity,
    rule_revision: StoredRevisionDigest,
    pure_digest: Option<StoredPureDigest>,
    action_digest: Option<StoredActionDigest>,
    observation_digest: Option<StoredObservationDigest>,
    action_spec_digest: Option<StoredActionSpecDigest>,
    action_spec: Option<Vec<u8>>,
    action_interface: Option<Vec<u8>>,
    observation_interface: Option<Vec<u8>>,
    observation_subject: Option<Vec<u8>>,
    observation_observer: Option<String>,
    authorization_denied: Option<bool>,
    authorization_policy: Option<String>,
    authorization_reason: Option<String>,
}

/// One input of the request an action computation was made from.
#[derive(Insertable, Queryable, Selectable)]
#[diesel(table_name = action_request_inputs)]
#[diesel(check_for_backend(Sqlite))]
struct ActionInputRow {
    computation: i64,
    position: i32,
    value: Vec<u8>,
}

#[derive(Insertable, Queryable, Selectable)]
#[diesel(table_name = observation_request_inputs)]
#[diesel(check_for_backend(Sqlite))]
struct ObservationInputRow {
    computation: i64,
    position: i32,
    value: Vec<u8>,
}

impl NewComputation {
    fn pure(key: PureComputationKey) -> Self {
        Self {
            rule_identity: StoredRuleIdentity(key.rule_identity),
            rule_revision: StoredRevisionDigest(key.rule_revision.digest()),
            pure_digest: Some(StoredPureDigest(key.digest)),
            action_digest: None,
            observation_digest: None,
            action_spec_digest: None,
            action_spec: None,
            action_interface: None,
            observation_interface: None,
            observation_subject: None,
            observation_observer: None,
            authorization_denied: None,
            authorization_policy: None,
            authorization_reason: None,
        }
    }

    fn action(
        computation_digest: ActionComputationDigest,
        request: &DurableActionRequest,
        plan: &DurableActionPlan,
        authorization: &ActionAuthorization,
    ) -> Self {
        let (denied, policy, reason) = match authorization {
            ActionAuthorization::Allowed { policy } => (false, policy.to_string(), None),
            ActionAuthorization::Denied { policy, reason } => {
                (true, policy.to_string(), Some(reason.to_string()))
            }
        };
        Self {
            rule_identity: StoredRuleIdentity(plan.rule().identity()),
            rule_revision: StoredRevisionDigest(plan.rule().revision().digest()),
            pure_digest: None,
            action_digest: Some(StoredActionDigest(computation_digest)),
            observation_digest: None,
            action_spec_digest: Some(StoredActionSpecDigest(plan.spec_digest())),
            action_spec: Some(plan.spec().encode_stored()),
            action_interface: Some(request.interface.encode_canonical()),
            observation_interface: None,
            observation_subject: None,
            observation_observer: None,
            authorization_denied: Some(denied),
            authorization_policy: Some(policy),
            authorization_reason: reason,
        }
    }

    fn observation(
        computation_digest: ObservationComputationDigest,
        request: &DurableObservationRequest,
        rule: &DurableRule,
        subject: &EncodedValue,
        observer: &str,
    ) -> Self {
        Self {
            rule_identity: StoredRuleIdentity(rule.identity()),
            rule_revision: StoredRevisionDigest(rule.revision().digest()),
            pure_digest: None,
            action_digest: None,
            observation_digest: Some(StoredObservationDigest(computation_digest)),
            action_spec_digest: None,
            action_spec: None,
            action_interface: None,
            observation_interface: Some(request.interface.encode_canonical()),
            observation_subject: Some(subject.as_bytes().to_vec()),
            observation_observer: Some(observer.to_string()),
            authorization_denied: None,
            authorization_policy: None,
            authorization_reason: None,
        }
    }
}

/// The identifier of the row for `computation`, inserting it when absent.
///
/// Pure computations are shared: many attempts name the same key, and the
/// unique index makes that one row. An action computation carries the
/// authorization decision of its own attempt, so it is never shared.
pub fn intern_computation(
    connection: &mut SqliteConnection,
    computation: &DurableComputation,
) -> Result<i64, Failure> {
    match computation {
        DurableComputation::Pure(key) => intern_pure_computation(connection, *key),
        DurableComputation::Action {
            computation_digest,
            request,
            plan,
            authorization,
        } => {
            let id: i64 = diesel::insert_into(computations::table)
                .values(NewComputation::action(
                    *computation_digest,
                    request,
                    plan,
                    authorization,
                ))
                .returning(computations::id)
                .get_result(connection)?;
            write_request_inputs(connection, id, request)?;
            Ok(id)
        }
        DurableComputation::Observation {
            computation_digest,
            request,
            rule,
            subject,
            observer,
        } => {
            let id: i64 = diesel::insert_into(computations::table)
                .values(NewComputation::observation(
                    *computation_digest,
                    request,
                    rule,
                    subject,
                    observer,
                ))
                .returning(computations::id)
                .get_result(connection)?;
            write_observation_request_inputs(connection, id, request)?;
            Ok(id)
        }
    }
}

fn write_observation_request_inputs(
    connection: &mut SqliteConnection,
    computation: i64,
    request: &DurableObservationRequest,
) -> Result<(), Failure> {
    if request.inputs.is_empty() {
        return Ok(());
    }
    let rows = request
        .inputs
        .iter()
        .enumerate()
        .map(|(index, value)| {
            Ok(ObservationInputRow {
                computation,
                position: i32::try_from(index).map_err(|_| {
                    corrupt("an observation request has more inputs than a position can name")
                })?,
                value: value.as_bytes().to_vec(),
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    diesel::insert_into(observation_request_inputs::table)
        .values(rows)
        .execute(connection)?;
    Ok(())
}

fn write_request_inputs(
    connection: &mut SqliteConnection,
    computation: i64,
    request: &DurableActionRequest,
) -> Result<(), Failure> {
    if request.inputs.is_empty() {
        return Ok(());
    }
    let mut rows = Vec::with_capacity(request.inputs.len());
    for (index, value) in request.inputs.iter().enumerate() {
        rows.push(ActionInputRow {
            computation,
            position: i32::try_from(index).map_err(|_| {
                corrupt("an action request has more inputs than a position can name")
            })?,
            value: value.as_bytes().to_vec(),
        });
    }
    diesel::insert_into(action_request_inputs::table)
        .values(rows)
        .execute(connection)?;
    Ok(())
}

fn load_request_inputs(
    connection: &mut SqliteConnection,
    computation: i64,
) -> Result<Box<[EncodedValue]>, Failure> {
    let rows: Vec<ActionInputRow> = action_request_inputs::table
        .filter(action_request_inputs::computation.eq(computation))
        .order(action_request_inputs::position.asc())
        .select(ActionInputRow::as_select())
        .load(connection)?;
    rows.into_iter()
        .map(|row| {
            EncodedValue::from_bytes(row.value).map_err(|error| {
                corrupt(format!(
                    "a stored action request input is unreadable: {error}"
                ))
            })
        })
        .collect()
}

fn load_observation_request_inputs(
    connection: &mut SqliteConnection,
    computation: i64,
) -> Result<Box<[EncodedValue]>, Failure> {
    let rows: Vec<ObservationInputRow> = observation_request_inputs::table
        .filter(observation_request_inputs::computation.eq(computation))
        .order(observation_request_inputs::position.asc())
        .select(ObservationInputRow::as_select())
        .load(connection)?;
    rows.into_iter()
        .map(|row| {
            EncodedValue::from_bytes(row.value).map_err(|error| {
                corrupt(format!(
                    "a stored observation request input is unreadable: {error}"
                ))
            })
        })
        .collect()
}

pub fn intern_pure_computation(
    connection: &mut SqliteConnection,
    key: PureComputationKey,
) -> Result<i64, Failure> {
    if let Some(id) = find_pure_computation(connection, key)? {
        return Ok(id);
    }
    Ok(diesel::insert_into(computations::table)
        .values(NewComputation::pure(key))
        .returning(computations::id)
        .get_result(connection)?)
}

pub fn find_pure_computation(
    connection: &mut SqliteConnection,
    key: PureComputationKey,
) -> Result<Option<i64>, Failure> {
    Ok(computations::table
        .filter(computations::rule_identity.eq(StoredRuleIdentity(key.rule_identity)))
        .filter(computations::rule_revision.eq(StoredRevisionDigest(key.rule_revision.digest())))
        .filter(computations::pure_digest.eq(Some(StoredPureDigest(key.digest))))
        .select(computations::id)
        .first(connection)
        .optional()?)
}

pub(super) fn load_computation(
    connection: &mut SqliteConnection,
    id: i64,
) -> Result<DurableComputation, Failure> {
    let row: ComputationRow = computations::table
        .find(id)
        .select(ComputationRow::as_select())
        .first(connection)?;
    row.into_computation(connection)
}

pub(super) fn load_pure_key(
    connection: &mut SqliteConnection,
    id: i64,
) -> Result<PureComputationKey, Failure> {
    match load_computation(connection, id)? {
        DurableComputation::Pure(key) => Ok(key),
        DurableComputation::Action { .. } | DurableComputation::Observation { .. } => {
            Err(corrupt("a pure edge references a non-pure computation"))
        }
    }
}

impl ComputationRow {
    fn revision(&self) -> RuleRevision {
        RuleRevision::from_parts(self.rule_identity.0, self.rule_revision.0)
    }

    /// The request inputs live in their own table, so rebuilding an action
    /// computation needs a second read. A pure key returns before it happens.
    fn into_computation(
        self,
        connection: &mut SqliteConnection,
    ) -> Result<DurableComputation, Failure> {
        if let Some(digest) = self.pure_digest {
            return Ok(DurableComputation::Pure(PureComputationKey {
                rule_identity: self.rule_identity.0,
                rule_revision: self.revision(),
                digest: digest.0,
            }));
        }
        let revision = self.revision();
        if let Some(observation_digest) = self.observation_digest {
            let encoded_interface = self.observation_interface.ok_or_else(|| {
                corrupt("an observation computation retains no request interface")
            })?;
            let interface = Interface::decode_canonical(&encoded_interface).map_err(|error| {
                corrupt(format!(
                    "a stored observation request interface is unreadable: {error}"
                ))
            })?;
            let subject = EncodedValue::from_bytes(
                self.observation_subject
                    .ok_or_else(|| corrupt("an observation computation retains no subject"))?,
            )
            .map_err(|error| {
                corrupt(format!(
                    "a stored observation subject is unreadable: {error}"
                ))
            })?;
            return Ok(DurableComputation::Observation {
                computation_digest: observation_digest.0,
                request: DurableObservationRequest {
                    interface,
                    inputs: load_observation_request_inputs(connection, self.id)?,
                },
                rule: DurableRule::new(revision),
                subject,
                observer: self
                    .observation_observer
                    .ok_or_else(|| corrupt("an observation computation retains no observer"))?
                    .into(),
            });
        }
        let (
            Some(action_digest),
            Some(stored_digest),
            Some(encoded),
            Some(encoded_interface),
            Some(denied),
            Some(policy),
        ) = (
            self.action_digest,
            self.action_spec_digest,
            self.action_spec,
            self.action_interface,
            self.authorization_denied,
            self.authorization_policy,
        )
        else {
            return Err(corrupt(format!(
                "computation {} is neither a pure key nor a complete action plan",
                self.id
            )));
        };
        let spec = ActionSpec::decode_stored(&encoded)
            .map_err(|error| corrupt(format!("a stored action contract is unreadable: {error}")))?;
        let plan = DurableActionPlan::new(DurableRule::new(revision), spec)
            .map_err(|_| corrupt("a stored action contract is not a valid declared contract"))?;
        if plan.spec_digest() != stored_digest.0 {
            return Err(corrupt(
                "a stored action contract does not reproduce its stored contract digest",
            ));
        }
        let authorization = if denied {
            ActionAuthorization::Denied {
                policy: policy.into(),
                reason: self
                    .authorization_reason
                    .ok_or_else(|| corrupt("a denied action retains no denial reason"))?
                    .into(),
            }
        } else {
            ActionAuthorization::Allowed {
                policy: policy.into(),
            }
        };
        let interface = Interface::decode_canonical(&encoded_interface).map_err(|error| {
            corrupt(format!(
                "a stored action request interface is unreadable: {error}"
            ))
        })?;
        Ok(DurableComputation::Action {
            computation_digest: action_digest.0,
            request: DurableActionRequest {
                interface,
                inputs: load_request_inputs(connection, self.id)?,
            },
            plan: Box::new(plan),
            authorization,
        })
    }
}
