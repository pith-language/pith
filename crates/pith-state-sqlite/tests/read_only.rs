//! Read-only engine-state access against a real filesystem.

use std::path::PathBuf;

use diesel::Connection as _;
use diesel::connection::SimpleConnection as _;
use diesel::sqlite::SqliteConnection;
use pith_core::{Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Type, Value};
use pith_diag::Span;
use pith_engine::state::{
    AttemptStatistics, CompletedAttempt, DurableComputation, DurableProvenance,
    DurableReuseDecision, EncodedValue, EngineStateReader, EngineStateStore,
};
use pith_state_sqlite::{ReadOnlySqliteEngineStateStore, SqliteEngineStateStore, SqliteStateError};

/// A scratch directory removed when the test ends, named from the process id
/// and the test label so a failed run leaves a predictable path to inspect.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("pith-sqlite-ro-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        if let Err(error) = std::fs::create_dir_all(&path) {
            unreachable!("could not create the scratch directory: {error}");
        }
        Self { path }
    }

    fn database(&self) -> PathBuf {
        self.path.join("engine-state.sqlite")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn leaf_interface() -> Interface {
    Interface {
        inputs: Box::new([]),
        output: Type::Int,
    }
}

fn leaf_rule() -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("pith-state-sqlite-fixture", "leaf");
    let revision = RuleRevision::of_manifest(identity, b"pith-state-sqlite-fixture-v1");
    Rule::<Pure>::new(
        "pith-state-sqlite-fixture",
        revision,
        "leaf",
        leaf_interface(),
        Span::none(),
    )
}

fn leaf_request() -> Request<Pure> {
    Request::<Pure>::new("leaf", leaf_interface(), [], Span::none())
}

fn leaf_computation() -> DurableComputation {
    DurableComputation::Pure(pith_core::PureComputationKey::new(
        &leaf_rule(),
        &leaf_request(),
    ))
}

/// What a writable open recorded is what a read-only open reads, including
/// pending attempts it must leave pending (recovery is a write).
#[test]
fn a_read_only_open_reads_what_the_writable_open_wrote() {
    let scratch = Scratch::new("reads-what-was-written");
    let database = scratch.database();

    let writable = SqliteEngineStateStore::open(&database)
        .unwrap_or_else(|error| unreachable!("could not open writable: {error}"));
    let pending = writable
        .create_pending_attempt(leaf_computation())
        .unwrap_or_else(|error| unreachable!("could not create an attempt: {error}"));
    drop(writable);

    let read_only = ReadOnlySqliteEngineStateStore::open_read_only(&database)
        .unwrap_or_else(|error| unreachable!("could not open read-only: {error}"));
    let pending_attempts = read_only
        .pending_attempts()
        .unwrap_or_else(|error| unreachable!("could not read pending attempts: {error}"));
    assert_eq!(pending_attempts.len(), 1, "the pending attempt is gone");
    let attempt = read_only
        .attempt(pending)
        .unwrap_or_else(|error| unreachable!("could not read the attempt: {error}"));
    assert!(
        attempt.is_some(),
        "the recorded attempt cannot be read back"
    );
}

/// Writes do not exist on the read-only type — they are not refused, they
/// are absent — so the only thing a test can still assert is the lattice:
/// the read-only store satisfies every reader bound, and the writable store
/// does too (read-write implies read-only through the supertrait).
#[test]
fn the_read_only_store_satisfies_every_reader_bound() {
    fn accepts_reader(reader: &dyn pith_engine::state::EngineStateReader) -> bool {
        reader.versions() == pith_state_sqlite::SqliteEngineStateStore::current_versions()
    }

    let scratch = Scratch::new("reader-bound");
    let database = scratch.database();

    let writable = SqliteEngineStateStore::open(&database)
        .unwrap_or_else(|error| unreachable!("could not open writable: {error}"));
    drop(writable);

    let read_only = ReadOnlySqliteEngineStateStore::open_read_only(&database)
        .unwrap_or_else(|error| unreachable!("could not open read-only: {error}"));
    assert!(accepts_reader(&read_only));

    let writable = SqliteEngineStateStore::open(&database)
        .unwrap_or_else(|error| unreachable!("could not open writable: {error}"));
    assert!(accepts_reader(&writable));
}

/// The enumeration a `Session<ReadOnly>` serves its inspection commands from:
/// the counts, every record through decode, and the root set the index names.
/// A completed attempt reaches the reusable index; the same key attempted
/// twice leaves one root, because the later publication supersedes it.
#[test]
fn a_read_only_open_enumerates_the_store() {
    let scratch = Scratch::new("enumerates");
    let database = scratch.database();

    let writable = SqliteEngineStateStore::open(&database)
        .unwrap_or_else(|error| unreachable!("could not open writable: {error}"));
    let completion = || CompletedAttempt {
        dependencies: Box::new([]),
        result: EncodedValue::from_value(&Value::Int(1.into())),
        provenance: DurableProvenance::Pure,
        reuse: DurableReuseDecision::Reusable,
        capabilities: Box::new([]),
    };
    for _ in 0..2 {
        let attempt = writable
            .create_pending_attempt(leaf_computation())
            .unwrap_or_else(|error| unreachable!("could not create an attempt: {error}"));
        writable
            .publish_complete(attempt, completion())
            .unwrap_or_else(|error| unreachable!("could not complete the attempt: {error}"));
    }
    drop(writable);

    let read_only = ReadOnlySqliteEngineStateStore::open_read_only(&database)
        .unwrap_or_else(|error| unreachable!("could not open read-only: {error}"));
    let statistics = read_only
        .attempt_statistics()
        .unwrap_or_else(|error| unreachable!("could not count: {error}"));
    assert_eq!(
        statistics,
        AttemptStatistics {
            attempts: 2,
            pending: 0,
            complete: 2,
            failed: 0,
            cancelled: 0,
            reusable_index: 1,
        },
        "two attempts of one key hold one index root"
    );
    let all = read_only
        .all_attempts()
        .unwrap_or_else(|error| unreachable!("could not enumerate: {error}"));
    assert_eq!(all.len(), 2, "the history holds both attempts");
    let roots = read_only
        .reusable_index_attempts()
        .unwrap_or_else(|error| unreachable!("could not read the index: {error}"));
    assert_eq!(roots.len(), 1, "the root set names the latest attempt only");
    let newest = all.last().unwrap_or_else(|| unreachable!("two attempts"));
    assert_eq!(
        roots.first().map(|root| root.id),
        Some(newest.id),
        "the surviving root is not the latest publication"
    );
}

/// A writable open creates the database; a read-only open must not, or a
/// mistyped path would silently become an empty cache.
#[test]
fn a_missing_database_cannot_be_opened_read_only() {
    let scratch = Scratch::new("missing-refused");
    let database = scratch.database().join("absent.sqlite");

    assert!(ReadOnlySqliteEngineStateStore::open_read_only(&database).is_err());
}

/// An empty or non-database file at the path is `NothingToRead`, not a quiet
/// success over zero attempts.
#[test]
fn an_empty_file_is_nothing_to_read() {
    let scratch = Scratch::new("empty-refused");
    let database = scratch.database();

    std::fs::write(&database, b"").unwrap_or_else(|error| unreachable!("could not write: {error}"));

    let refused = ReadOnlySqliteEngineStateStore::open_read_only(&database);
    assert!(matches!(
        refused,
        Err(SqliteStateError::NothingToRead { .. })
    ));
}

/// A read-only open refuses incompatible versions instead of rebuilding.
#[test]
fn an_incompatible_database_is_refused_read_only() {
    let scratch = Scratch::new("incompatible-refused");
    let database = scratch.database();

    let writable = SqliteEngineStateStore::open(&database)
        .unwrap_or_else(|error| unreachable!("could not open writable: {error}"));
    drop(writable);

    let url = match database.to_str() {
        Some(url) => url,
        None => unreachable!("the scratch path is not utf-8"),
    };
    let mut connection = SqliteConnection::establish(url)
        .unwrap_or_else(|error| unreachable!("could not open to corrupt: {error}"));
    connection
        .batch_execute("update engine_state_versions set schema_version = 99 where id = 0;")
        .unwrap_or_else(|error| unreachable!("could not corrupt the versions: {error}"));
    drop(connection);

    let refused = ReadOnlySqliteEngineStateStore::open_read_only(&database);
    assert!(matches!(
        refused,
        Err(SqliteStateError::IncompatibleReadOnly { .. })
    ));
}

/// The read-only URI is built by percent-encoding the path, so a database in a
/// directory whose name carries a character the URI grammar would otherwise
/// read (a space, a `?`) still opens.
#[test]
fn a_path_with_uri_characters_opens_read_only() {
    let label = "uri path ? and space";
    let scratch = Scratch::new(label);
    let database = scratch.path.join(label).join("engine-state.sqlite");
    std::fs::create_dir_all(
        database
            .parent()
            .unwrap_or_else(|| unreachable!("no parent")),
    )
    .unwrap_or_else(|error| unreachable!("could not create the directory: {error}"));

    let writable = SqliteEngineStateStore::open(&database)
        .unwrap_or_else(|error| unreachable!("could not open writable: {error}"));
    drop(writable);

    let read_only = ReadOnlySqliteEngineStateStore::open_read_only(&database);
    assert!(
        read_only.is_ok(),
        "a URI-significant path did not open: {:?}",
        read_only.err().map(|error| error.to_string())
    );
}

#[test]
fn a_dangling_reusable_index_entry_is_reported_as_corruption() {
    let scratch = Scratch::new("dangling-index");
    let database = scratch.database();
    let writable = SqliteEngineStateStore::open(&database)
        .unwrap_or_else(|error| unreachable!("could not open writable: {error}"));
    let attempt = writable
        .create_pending_attempt(leaf_computation())
        .unwrap_or_else(|error| unreachable!("could not create an attempt: {error}"));
    writable
        .publish_complete(
            attempt,
            CompletedAttempt {
                dependencies: Box::new([]),
                result: EncodedValue::from_value(&Value::Int(1.into())),
                provenance: DurableProvenance::Pure,
                reuse: DurableReuseDecision::Reusable,
                capabilities: Box::new([]),
            },
        )
        .unwrap_or_else(|error| unreachable!("could not complete the attempt: {error}"));
    drop(writable);

    let url = database
        .to_str()
        .unwrap_or_else(|| unreachable!("the scratch path is not utf-8"));
    let mut connection = SqliteConnection::establish(url)
        .unwrap_or_else(|error| unreachable!("could not open to corrupt: {error}"));
    connection
        .batch_execute("pragma foreign_keys = off; update reusable_index set attempt = 999999;")
        .unwrap_or_else(|error| unreachable!("could not corrupt the index: {error}"));
    drop(connection);

    let read_only = ReadOnlySqliteEngineStateStore::open_read_only(&database)
        .unwrap_or_else(|error| unreachable!("could not open read-only: {error}"));
    assert!(read_only.attempt_statistics().is_err());
    assert!(read_only.reusable_index_attempts().is_err());
}
