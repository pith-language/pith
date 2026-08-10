//! Ordered dependency edges. A row's position in its attempt is semantic.

use pith_core::CapabilityRequirement;
use pith_engine::state::DurableDependency;

use crate::columns::{DependencyKind, StoredAttemptId, StoredContentId, StoredDependencyKind};
use crate::schema::dependencies;

use super::computation::{intern_pure_computation, load_pure_key};
use super::{corrupt, position};
use diesel::prelude::*;
use diesel::sqlite::Sqlite;

use super::Failure;

#[derive(Insertable)]
#[diesel(table_name = dependencies)]
struct NewDependency {
    attempt: StoredAttemptId,
    position: i32,
    kind: StoredDependencyKind,
    pure_computation: Option<i64>,
    dependency_attempt: Option<StoredAttemptId>,
    content: Option<StoredContentId>,
    capability_name: Option<String>,
    capability_scope: Option<String>,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = dependencies)]
#[diesel(check_for_backend(Sqlite))]
struct DependencyRow {
    kind: StoredDependencyKind,
    pure_computation: Option<i64>,
    dependency_attempt: Option<StoredAttemptId>,
    content: Option<StoredContentId>,
    capability_name: Option<String>,
    capability_scope: Option<String>,
}

pub(super) fn write_dependencies(
    connection: &mut SqliteConnection,
    attempt: StoredAttemptId,
    edges: &[DurableDependency],
) -> Result<(), Failure> {
    let mut rows = Vec::with_capacity(edges.len());
    for (index, edge) in edges.iter().enumerate() {
        let position = position(index)?;
        rows.push(match edge {
            DurableDependency::Pure {
                computation,
                attempt: target,
            } => NewDependency {
                attempt,
                position,
                kind: StoredDependencyKind(DependencyKind::Pure),
                pure_computation: Some(intern_pure_computation(connection, *computation)?),
                dependency_attempt: Some(StoredAttemptId(*target)),
                content: None,
                capability_name: None,
                capability_scope: None,
            },
            DurableDependency::Action { attempt: target } => NewDependency {
                attempt,
                position,
                kind: StoredDependencyKind(DependencyKind::Action),
                pure_computation: None,
                dependency_attempt: Some(StoredAttemptId(*target)),
                content: None,
                capability_name: None,
                capability_scope: None,
            },
            DurableDependency::Blob { content } => NewDependency {
                attempt,
                position,
                kind: StoredDependencyKind(DependencyKind::Blob),
                pure_computation: None,
                dependency_attempt: None,
                content: Some(StoredContentId(*content)),
                capability_name: None,
                capability_scope: None,
            },
            DurableDependency::CapabilityUse { capability } => NewDependency {
                attempt,
                position,
                kind: StoredDependencyKind(DependencyKind::CapabilityUse),
                pure_computation: None,
                dependency_attempt: None,
                content: None,
                capability_name: Some(capability.name.to_string()),
                capability_scope: Some(capability.scope.to_string()),
            },
        });
    }
    diesel::insert_into(dependencies::table)
        .values(rows)
        .execute(connection)?;
    Ok(())
}

pub(super) fn load_dependencies(
    connection: &mut SqliteConnection,
    attempt: StoredAttemptId,
) -> Result<Box<[DurableDependency]>, Failure> {
    let rows: Vec<DependencyRow> = dependencies::table
        .filter(dependencies::attempt.eq(attempt))
        .order(dependencies::position.asc())
        .select(DependencyRow::as_select())
        .load(connection)?;
    let mut edges = Vec::with_capacity(rows.len());
    for row in rows {
        let missing = |field: &str| corrupt(format!("a dependency row has no {field}"));
        edges.push(match row.kind.0 {
            DependencyKind::Pure => DurableDependency::Pure {
                computation: load_pure_key(
                    connection,
                    row.pure_computation.ok_or_else(|| missing("computation"))?,
                )?,
                attempt: row.dependency_attempt.ok_or_else(|| missing("attempt"))?.0,
            },
            DependencyKind::Action => DurableDependency::Action {
                attempt: row.dependency_attempt.ok_or_else(|| missing("attempt"))?.0,
            },
            DependencyKind::Blob => DurableDependency::Blob {
                content: row.content.ok_or_else(|| missing("content identity"))?.0,
            },
            DependencyKind::CapabilityUse => DurableDependency::CapabilityUse {
                capability: CapabilityRequirement {
                    name: row.capability_name.ok_or_else(|| missing("name"))?.into(),
                    scope: row.capability_scope.ok_or_else(|| missing("scope"))?.into(),
                },
            },
        });
    }
    Ok(edges.into_boxed_slice())
}
