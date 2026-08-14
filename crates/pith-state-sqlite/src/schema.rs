//! The table layout and its version gate.

use pith_engine::state::{EngineStateVersions, RECORD_ENCODING_VERSION, SchemaVersion};

/// Version of the table layout below. Independent of the record encoding
/// version, which belongs to `pith-engine`: the same records can be laid out
/// differently, and the same layout can hold a new payload encoding.
///
/// Pinned at 1 until something is released, per decision 0024: nothing has
/// shipped, so no database outside a working tree needs to survive a change,
/// the schema is free to follow the engine's shape, and an incompatible
/// database is moved aside and rebuilt — starting clean is the intended
/// behaviour here, not a fallback. Once there is a release to be compatible
/// with, this starts moving for any change a prior build would misread: a
/// new table or column, and equally a new code in an existing column —
/// adding `Cancelled` to `attempts.status` left the layout untouched but
/// would have had an older build pass the gate and then fail decoding a
/// variant it does not have.
pub const SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1);

pub const CURRENT_VERSIONS: EngineStateVersions = EngineStateVersions {
    schema: SCHEMA_VERSION,
    semantic_encoding: RECORD_ENCODING_VERSION,
};

/// A completed pure result and a declared action contract are stored as the
/// canonical bytes `pith-core` gives them, because both carry a digest that
/// those exact bytes must reproduce. Everything else is columns and rows
/// (decision 0025).
pub const CREATE_SCHEMA: &str = "
    create table if not exists engine_state_versions (
        id integer primary key check (id = 0),
        schema_version integer not null,
        semantic_encoding_version integer not null
    ) strict;

    create table if not exists computations (
        id integer primary key autoincrement,
        rule_identity blob not null,
        rule_revision blob not null,
        pure_digest blob,
        action_digest blob,
        action_spec_digest blob,
        action_spec blob,
        action_interface blob,
        authorization_denied integer,
        authorization_policy text,
        authorization_reason text
    ) strict;

    -- The inputs of the request an action computation was made from, in
    -- request order (decision 0033). Each is the canonical bytes of one typed
    -- value, because rebuilding the computation digest has to reproduce them.
    create table if not exists action_request_inputs (
        computation integer not null references computations (id),
        position integer not null,
        value blob not null,
        primary key (computation, position)
    ) strict;

    create unique index if not exists computations_pure_key
        on computations (rule_identity, rule_revision, pure_digest)
        where pure_digest is not null;

    -- Not unique: an action computation row carries the authorization decision
    -- of its own attempt, so one action key has a row per attempt. Reading the
    -- reusable index needs all of them (decision 0031).
    create index if not exists computations_action_key
        on computations (rule_identity, rule_revision, action_digest)
        where action_digest is not null;

    create table if not exists attempts (
        id integer primary key autoincrement,
        computation integer not null references computations (id),
        status integer not null,
        result blob,
        reuse integer,
        reuse_attempt integer references attempts (id),
        reuse_computation integer references computations (id),
        provenance integer,
        executor text,
        platform_operating_system text,
        platform_architecture text,
        access integer
    ) strict;

    create index if not exists attempts_by_computation on attempts (computation, id);
    create index if not exists attempts_pending on attempts (id) where status = 0;

    create table if not exists dependencies (
        attempt integer not null references attempts (id),
        position integer not null,
        kind integer not null,
        pure_computation integer references computations (id),
        dependency_attempt integer references attempts (id),
        content blob,
        capability_name text,
        capability_scope text,
        primary key (attempt, position)
    ) strict;

    create index if not exists dependencies_by_target
        on dependencies (dependency_attempt)
        where dependency_attempt is not null;

    create table if not exists report_capabilities (
        attempt integer not null references attempts (id),
        position integer not null,
        name text not null,
        scope text not null,
        primary key (attempt, position)
    ) strict;

    -- What a completed attempt requires, as opposed to what its executor
    -- reported using (decision 0033). Separate from `report_capabilities`
    -- because a pure attempt has requirements and no report.
    create table if not exists required_capabilities (
        attempt integer not null references attempts (id),
        position integer not null,
        name text not null,
        scope text not null,
        primary key (attempt, position)
    ) strict;

    create table if not exists produced_outputs (
        attempt integer not null references attempts (id),
        position integer not null,
        path text not null,
        kind integer not null,
        content blob,
        primary key (attempt, position)
    ) strict;

    create table if not exists diagnostics (
        id integer primary key autoincrement,
        attempt integer not null references attempts (id),
        position integer not null,
        severity integer not null,
        code integer not null,
        span_start integer not null,
        span_end integer not null,
        message text not null
    ) strict;

    create unique index if not exists diagnostics_order on diagnostics (attempt, position);

    create table if not exists diagnostic_notes (
        diagnostic integer not null references diagnostics (id),
        position integer not null,
        span_start integer not null,
        span_end integer not null,
        message text not null,
        primary key (diagnostic, position)
    ) strict;

    -- The reusable index. `published` is the database's publication
    -- sequence, so latest reads as latest published: two attempts of one
    -- action key can complete in either order of their creation, and the
    -- admission test of 0031 serves the most recently recorded attempt.
    create table if not exists reusable_index (
        computation integer primary key references computations (id),
        attempt integer not null references attempts (id),
        published integer not null
    ) strict;
";

diesel::table! {
    engine_state_versions (id) {
        id -> Integer,
        schema_version -> Integer,
        semantic_encoding_version -> Integer,
    }
}

diesel::table! {
    computations (id) {
        id -> BigInt,
        rule_identity -> Binary,
        rule_revision -> Binary,
        pure_digest -> Nullable<Binary>,
        action_digest -> Nullable<Binary>,
        action_spec_digest -> Nullable<Binary>,
        action_spec -> Nullable<Binary>,
        action_interface -> Nullable<Binary>,
        authorization_denied -> Nullable<Bool>,
        authorization_policy -> Nullable<Text>,
        authorization_reason -> Nullable<Text>,
    }
}

diesel::table! {
    attempts (id) {
        id -> BigInt,
        computation -> BigInt,
        status -> Integer,
        result -> Nullable<Binary>,
        reuse -> Nullable<Integer>,
        reuse_attempt -> Nullable<BigInt>,
        reuse_computation -> Nullable<BigInt>,
        provenance -> Nullable<Integer>,
        executor -> Nullable<Text>,
        platform_operating_system -> Nullable<Text>,
        platform_architecture -> Nullable<Text>,
        access -> Nullable<Integer>,
    }
}

diesel::table! {
    dependencies (attempt, position) {
        attempt -> BigInt,
        position -> Integer,
        kind -> Integer,
        pure_computation -> Nullable<BigInt>,
        dependency_attempt -> Nullable<BigInt>,
        content -> Nullable<Binary>,
        capability_name -> Nullable<Text>,
        capability_scope -> Nullable<Text>,
    }
}

diesel::table! {
    action_request_inputs (computation, position) {
        computation -> BigInt,
        position -> Integer,
        value -> Binary,
    }
}

diesel::table! {
    required_capabilities (attempt, position) {
        attempt -> BigInt,
        position -> Integer,
        name -> Text,
        scope -> Text,
    }
}

diesel::table! {
    report_capabilities (attempt, position) {
        attempt -> BigInt,
        position -> Integer,
        name -> Text,
        scope -> Text,
    }
}

diesel::table! {
    produced_outputs (attempt, position) {
        attempt -> BigInt,
        position -> Integer,
        path -> Text,
        kind -> Integer,
        content -> Nullable<Binary>,
    }
}

diesel::table! {
    diagnostics (id) {
        id -> BigInt,
        attempt -> BigInt,
        position -> Integer,
        severity -> Integer,
        code -> Integer,
        span_start -> Integer,
        span_end -> Integer,
        message -> Text,
    }
}

diesel::table! {
    diagnostic_notes (diagnostic, position) {
        diagnostic -> BigInt,
        position -> Integer,
        span_start -> Integer,
        span_end -> Integer,
        message -> Text,
    }
}

diesel::table! {
    reusable_index (computation) {
        computation -> BigInt,
        attempt -> BigInt,
        published -> BigInt,
    }
}

diesel::joinable!(attempts -> computations (computation));
diesel::joinable!(reusable_index -> computations (computation));
diesel::allow_tables_to_appear_in_same_query!(
    action_request_inputs,
    attempts,
    computations,
    dependencies,
    diagnostic_notes,
    diagnostics,
    produced_outputs,
    report_capabilities,
    required_capabilities,
    reusable_index,
);
