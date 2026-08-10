//! Storage types for the kernel's identities and fieldless enums.
//!
//! Each digest-bearing identity gets its own column type, so a rule identity
//! cannot be bound where a content identity belongs even though both are 32
//! bytes. Each enum's integer code is written once and read back through the
//! same table, so adding a variant fails to compile rather than storing a code
//! nothing reads.

use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::{BigInt, Binary, Integer};
use diesel::sqlite::Sqlite;
use pith_core::OutputKind;
use pith_diag::Severity;
use pith_engine::AccessVerification;
use pith_engine::state::{DurableAttemptId, DurableAttemptStatus};
use pith_ids::{
    ActionSpecDigest, ContentDigest, ContentId, DIGEST_LEN, PureComputationDigest, RuleIdentity,
};

fn digest_from_bytes(bytes: &[u8]) -> deserialize::Result<ContentDigest> {
    let bytes = <[u8; DIGEST_LEN]>::try_from(bytes)
        .map_err(|_| format!("expected a {DIGEST_LEN}-byte digest, found {}", bytes.len()))?;
    Ok(ContentDigest::from_bytes(bytes))
}

macro_rules! digest_column {
    ($wrapper:ident, $inner:ty, $restore:expr, $extract:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
        #[diesel(sql_type = Binary)]
        pub struct $wrapper(pub $inner);

        impl ToSql<Binary, Sqlite> for $wrapper {
            fn to_sql<'store>(
                &'store self,
                out: &mut Output<'store, '_, Sqlite>,
            ) -> serialize::Result {
                let extract: fn(&$inner) -> ContentDigest = $extract;
                out.set_value(extract(&self.0).as_bytes().to_vec());
                Ok(IsNull::No)
            }
        }

        impl FromSql<Binary, Sqlite> for $wrapper {
            fn from_sql(value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
                let restore: fn(ContentDigest) -> $inner = $restore;
                let bytes = <Vec<u8> as FromSql<Binary, Sqlite>>::from_sql(value)?;
                Ok(Self(restore(digest_from_bytes(&bytes)?)))
            }
        }
    };
}

digest_column!(
    StoredRuleIdentity,
    RuleIdentity,
    RuleIdentity::from_digest,
    |identity| identity.digest()
);
digest_column!(
    StoredPureDigest,
    PureComputationDigest,
    PureComputationDigest::from_digest,
    |digest| digest.digest()
);
digest_column!(
    StoredActionSpecDigest,
    ActionSpecDigest,
    ActionSpecDigest::from_digest,
    |digest| digest.digest()
);
digest_column!(
    StoredContentId,
    ContentId,
    ContentId::from_digest,
    |content| content.digest()
);
// A rule revision is derived from its identity as well as its digest, so it is
// stored as a bare digest and rejoined with the identity when a row is read.
digest_column!(
    StoredRevisionDigest,
    ContentDigest,
    |digest| digest,
    |digest| *digest
);

macro_rules! stored_enum {
    ($wrapper:ident, $inner:ty, { $($variant:path => $code:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
        #[diesel(sql_type = Integer)]
        pub struct $wrapper(pub $inner);

        impl $wrapper {
            const fn code(self) -> i32 {
                match self.0 {
                    $($variant => $code,)+
                }
            }

            fn from_code(code: i32) -> Option<Self> {
                match code {
                    $($code => Some(Self($variant)),)+
                    _ => None,
                }
            }
        }

        impl ToSql<Integer, Sqlite> for $wrapper {
            fn to_sql<'store>(
                &'store self,
                out: &mut Output<'store, '_, Sqlite>,
            ) -> serialize::Result {
                out.set_value(self.code());
                Ok(IsNull::No)
            }
        }

        impl FromSql<Integer, Sqlite> for $wrapper {
            fn from_sql(value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
                let code = <i32 as FromSql<Integer, Sqlite>>::from_sql(value)?;
                Self::from_code(code).ok_or_else(|| {
                    format!("{} has no variant for code {code}", stringify!($inner)).into()
                })
            }
        }
    };
}

stored_enum!(StoredStatus, DurableAttemptStatus, {
    DurableAttemptStatus::Pending => 0,
    DurableAttemptStatus::Complete => 1,
    DurableAttemptStatus::Failed => 2,
});

stored_enum!(StoredSeverity, Severity, {
    Severity::Error => 0,
    Severity::Warning => 1,
    Severity::Info => 2,
    Severity::Note => 3,
});

stored_enum!(StoredAccess, AccessVerification, {
    AccessVerification::Prevented => 0,
    AccessVerification::Observed => 1,
    AccessVerification::Unverified => 2,
});

stored_enum!(StoredOutputKind, OutputKind, {
    OutputKind::Blob => 0,
    OutputKind::Tree => 1,
});

/// Which shape a dependency row carries. The durable enum holds data, so the
/// discriminant is its own type here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Pure,
    Action,
    Blob,
    CapabilityUse,
}

stored_enum!(StoredDependencyKind, DependencyKind, {
    DependencyKind::Pure => 0,
    DependencyKind::Action => 1,
    DependencyKind::Blob => 2,
    DependencyKind::CapabilityUse => 3,
});

/// Which provenance an attempt row carries, and for an action whether the
/// engine imported what the executor captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceKind {
    Pure,
    ActionNotExecuted,
    ActionCaptured,
    ActionImported,
}

stored_enum!(StoredProvenanceKind, ProvenanceKind, {
    ProvenanceKind::Pure => 0,
    ProvenanceKind::ActionNotExecuted => 1,
    ProvenanceKind::ActionCaptured => 2,
    ProvenanceKind::ActionImported => 3,
});

/// A completed attempt's reuse decision, flattened to its discriminant. The
/// attempt or computation a reason names lives in its own column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReuseKind {
    Reusable,
    ActionCachingDisabled,
    DependencyPending,
    DependencyNotReusable,
    DependencyMissing,
}

stored_enum!(StoredReuseKind, ReuseKind, {
    ReuseKind::Reusable => 0,
    ReuseKind::ActionCachingDisabled => 1,
    ReuseKind::DependencyPending => 2,
    ReuseKind::DependencyNotReusable => 3,
    ReuseKind::DependencyMissing => 4,
});

/// Attempt identifiers are allocated by sqlite as `i64` and exposed as `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = BigInt)]
pub struct StoredAttemptId(pub DurableAttemptId);

impl ToSql<BigInt, Sqlite> for StoredAttemptId {
    fn to_sql<'store>(&'store self, out: &mut Output<'store, '_, Sqlite>) -> serialize::Result {
        out.set_value(i64::try_from(self.0.to_raw())?);
        Ok(IsNull::No)
    }
}

impl FromSql<BigInt, Sqlite> for StoredAttemptId {
    fn from_sql(value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let raw = <i64 as FromSql<BigInt, Sqlite>>::from_sql(value)?;
        Ok(Self(DurableAttemptId::from_raw(u64::try_from(raw)?)))
    }
}
