//! Typed output records and the renderer trait.
//!
//! Library code produces [`OutputRecord`] values and hands them to a
//! [`Renderer`]. The CLI picks one renderer from `--output {pretty,plain,json}`
//! and TTY detection. This is a leaf crate: it depends on `serde` and nothing
//! else in the pith stack. Rich types implement [`IntoOutput`] next to their
//! own definition so call sites read `sink.emit(x.to_record())`.

use std::io;

#[derive(Clone, Debug, serde::Serialize)]
pub struct OutputRecord {
    pub kind: RecordKind,
    pub code: u32,
    #[serde(flatten)]
    pub payload: Payload,
}

/// Stable string tag on every record. Never renumber, only add (K-11).
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Phase,
    Cache,
    Explain,
    Result,
    Summary,
    Query,
    Custom,
}

impl RecordKind {
    /// The stable snake_case tag for this kind. This is the single source of
    /// truth for the string every renderer (plain, pretty, JSON) uses; it must
    /// match the `#[serde(rename_all = "snake_case")]` derivation, which the
    /// `record_kind_as_str_matches_serde` test pins.
    pub const fn as_str(self) -> &'static str {
        match self {
            RecordKind::Phase => "phase",
            RecordKind::Cache => "cache",
            RecordKind::Explain => "explain",
            RecordKind::Result => "result",
            RecordKind::Summary => "summary",
            RecordKind::Query => "query",
            RecordKind::Custom => "custom",
        }
    }
}

/// Flattened into the parent `OutputRecord` JSON via `serde(flatten)`. Renaming
/// a variant or field is a breaking change for JSON consumers.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "detail", rename_all = "snake_case")]
pub enum Payload {
    Phase {
        name: Box<str>,
        status: PhaseStatus,
    },
    Cache {
        outcome: CacheOutcome,
    },
    Explain {
        steps: Box<[ExplainStep]>,
    },
    Result {
        summary: Box<str>,
    },
    Summary {
        hits: u64,
        misses: u64,
        reuses: u64,
        errors: u64,
        wall_ms: u64,
    },
    /// A typed answer from the query API. The version rides on the record
    /// rather than being announced once per run, so a reader that consumes one
    /// line in isolation still knows the contract it is reading.
    ///
    /// The view is nested rather than flattened. A DTO is free to name a field
    /// `kind` or `code`, and flattening one into the envelope would let it
    /// overwrite the envelope's own field of that name — silently, and
    /// differently per view.
    Query {
        api_version: u32,
        query: dto::QueryView,
    },
}

impl Payload {
    /// The [`RecordKind`] that classifies this payload. This is the single
    /// source of truth for the kind/payload pairing: an `OutputRecord`'s `kind`
    /// field is derived from its payload, never hand-paired at a construction
    /// site. The `kind_matches_payload_in_every_constructor` test pins this.
    pub const fn record_kind(&self) -> RecordKind {
        match self {
            Payload::Phase { .. } => RecordKind::Phase,
            Payload::Cache { .. } => RecordKind::Cache,
            Payload::Explain { .. } => RecordKind::Explain,
            Payload::Result { .. } => RecordKind::Result,
            Payload::Summary { .. } => RecordKind::Summary,
            Payload::Query { .. } => RecordKind::Query,
        }
    }
}

#[derive(Copy, Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Started,
    Finished,
    Failed,
}

impl PhaseStatus {
    /// Stable snake_case name; single source of truth for renderers.
    pub const fn as_str(self) -> &'static str {
        match self {
            PhaseStatus::Started => "started",
            PhaseStatus::Finished => "finished",
            PhaseStatus::Failed => "failed",
        }
    }
}

#[derive(Copy, Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheOutcome {
    Hit,
    Miss,
    Reuse,
}

impl CacheOutcome {
    /// Stable snake_case name; single source of truth for renderers.
    pub const fn as_str(self) -> &'static str {
        match self {
            CacheOutcome::Hit => "hit",
            CacheOutcome::Miss => "miss",
            CacheOutcome::Reuse => "reuse",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ExplainStep {
    pub label: Box<str>,
    pub detail: Box<str>,
}

impl OutputRecord {
    fn from_payload(payload: Payload) -> Self {
        Self {
            kind: payload.record_kind(),
            code: 0,
            payload,
        }
    }

    pub fn phase(name: impl Into<Box<str>>, status: PhaseStatus) -> Self {
        Self::from_payload(Payload::Phase {
            name: name.into(),
            status,
        })
    }

    pub fn cache(outcome: CacheOutcome) -> Self {
        Self::from_payload(Payload::Cache { outcome })
    }

    pub fn explain(steps: impl Into<Box<[ExplainStep]>>) -> Self {
        Self::from_payload(Payload::Explain {
            steps: steps.into(),
        })
    }

    /// Build a result record carrying a human-readable summary. Rich types that
    /// produce a summary implement [`IntoOutput`] rather than calling this
    /// directly.
    pub fn result(summary: impl Into<Box<str>>) -> Self {
        Self::from_payload(Payload::Result {
            summary: summary.into(),
        })
    }

    /// Build a query record, stamping the DTO contract's current version. The
    /// version is never passed in, so a caller cannot claim a shape it did not
    /// produce.
    pub fn query(view: dto::QueryView) -> Self {
        Self::from_payload(Payload::Query {
            api_version: dto::QUERY_API_VERSION,
            query: view,
        })
    }

    pub fn summary(hits: u64, misses: u64, reuses: u64, errors: u64, wall_ms: u64) -> Self {
        Self::from_payload(Payload::Summary {
            hits,
            misses,
            reuses,
            errors,
            wall_ms,
        })
    }
}

/// Render [`OutputRecord`]s or an explicitly raw command payload to bytes.
pub trait Renderer {
    /// # Errors
    /// Returns the underlying writer error if writing fails.
    fn emit(&mut self, out: &OutputRecord) -> io::Result<()>;

    /// Write bytes that must not be mixed with record framing.
    ///
    /// # Errors
    /// Returns the underlying writer error if writing fails.
    fn write_raw(&mut self, bytes: &[u8]) -> io::Result<()>;

    /// # Errors
    /// Returns the underlying writer error if flushing fails.
    fn finish(&mut self) -> io::Result<()>;
}

/// Implemented by rich types so call sites read `sink.emit(x.to_record())`.
pub trait IntoOutput {
    fn to_record(&self) -> OutputRecord;
}

pub struct Sink<R: Renderer> {
    renderer: R,
}

impl<R: Renderer> Sink<R> {
    pub fn new(renderer: R) -> Self {
        Self { renderer }
    }

    /// # Errors
    /// Returns the renderer's error if writing fails.
    pub fn emit(&mut self, out: &OutputRecord) -> io::Result<()> {
        self.renderer.emit(out)
    }

    /// # Errors
    /// Returns the renderer's error if writing fails.
    pub fn write_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.renderer.write_raw(bytes)
    }

    /// # Errors
    /// Returns the renderer's error if flushing fails.
    pub fn finish(mut self) -> io::Result<()> {
        self.renderer.finish()
    }
}

mod describe;
pub mod dto;
mod json;
pub mod palette;
mod plain;
mod pretty;

pub use json::JsonRenderer;
pub use plain::PlainRenderer;
pub use pretty::PrettyRenderer;

#[derive(Copy, Clone, Debug)]
pub enum OutputShape {
    Pretty,
    Plain,
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_record_shape_is_stable() {
        let rec = OutputRecord::summary(3, 2, 1, 0, 42);
        let json = serde_json::to_value(&rec).unwrap();
        insta::assert_json_snapshot!(json);
    }

    #[test]
    fn cache_outcome_serializes_as_snake_case() {
        let rec = OutputRecord::cache(CacheOutcome::Reuse);
        let json = serde_json::to_value(&rec).unwrap();
        assert_eq!(
            json.get("kind").and_then(serde_json::Value::as_str),
            Some("cache")
        );
        assert_eq!(
            json.get("outcome").and_then(serde_json::Value::as_str),
            Some("reuse")
        );
    }

    /// `as_str` is the single source of truth for renderer strings, but serde
    /// derives its own snake_case names independently. If they drift, JSON and
    /// the plain/pretty renderers will disagree. This pins them together.
    #[test]
    fn as_str_matches_serde_derivation() {
        for kind in [
            RecordKind::Phase,
            RecordKind::Cache,
            RecordKind::Explain,
            RecordKind::Result,
            RecordKind::Summary,
            RecordKind::Custom,
        ] {
            let json = serde_json::to_value(kind).unwrap();
            assert_eq!(
                json.as_str(),
                Some(kind.as_str()),
                "RecordKind as_str drifted from serde"
            );
        }
        for status in [
            PhaseStatus::Started,
            PhaseStatus::Finished,
            PhaseStatus::Failed,
        ] {
            let json = serde_json::to_value(status).unwrap();
            assert_eq!(
                json.as_str(),
                Some(status.as_str()),
                "PhaseStatus as_str drifted from serde"
            );
        }
        for outcome in [CacheOutcome::Hit, CacheOutcome::Miss, CacheOutcome::Reuse] {
            let json = serde_json::to_value(outcome).unwrap();
            assert_eq!(
                json.as_str(),
                Some(outcome.as_str()),
                "CacheOutcome as_str drifted from serde"
            );
        }
    }

    /// Every constructor must pair its `kind` with the matching `Payload`
    /// variant. Because `kind` is now derived from the payload, this also
    /// guards that the `kind` field never drifts from `payload.record_kind()`.
    #[test]
    fn kind_matches_payload_in_every_constructor() {
        let records = [
            OutputRecord::phase("build", PhaseStatus::Started),
            OutputRecord::cache(CacheOutcome::Hit),
            OutputRecord::explain([ExplainStep {
                label: "x".into(),
                detail: "y".into(),
            }]),
            OutputRecord::result("done"),
            OutputRecord::summary(0, 0, 0, 0, 0),
        ];
        for rec in records {
            assert_eq!(
                rec.kind,
                rec.payload.record_kind(),
                "constructor paired kind with the wrong payload variant"
            );
        }
    }
}
