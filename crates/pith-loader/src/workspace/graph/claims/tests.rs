mod generated;

use std::cell::Cell;

use super::*;
use pith_diag::SourceId;

struct Locator {
    location: Cell<Option<u8>>,
    calls: Cell<usize>,
}

impl ProjectStore for Locator {
    type Location = u8;

    fn locate(&self, _: &u8, _: &Route<'_>) -> Result<u8, AcquireFailure> {
        self.calls.set(self.calls.get().saturating_add(1));
        self.location
            .get()
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: "reference refused".into(),
            })
    }

    fn project(&mut self, _: &u8) -> Result<crate::AcquiredProject, AcquireFailure> {
        unreachable!("claim agreement does not acquire project bytes")
    }

    fn include(&mut self, _: &u8, _: &str) -> Result<crate::AcquiredSource, AcquireFailure> {
        unreachable!("claim agreement does not acquire include bytes")
    }
}

struct Request {
    subject: ModuleSubject,
    source: Arc<SourceFile>,
}

impl Request {
    fn new() -> Self {
        Self {
            subject: ModuleSubject::parse("example/dep")
                .unwrap_or_else(|_| unreachable!("valid subject")),
            source: Arc::new(SourceFile::new(SourceId::from_raw(0), "pith.pi", "")),
        }
    }

    fn route(&self, archive: bool) -> Route<'_> {
        if archive {
            Route::Archive {
                url: "archive",
                digest: "unvalidated",
            }
        } else {
            Route::Git {
                url: "repository",
                revision: "main",
                subpath: None,
            }
        }
    }

    /// What the walk does for one input: locate its route, then claim the
    /// subject the acquired project declared.
    fn locate(
        &self,
        claims: &mut Claims<u8>,
        store: &Locator,
        archive: bool,
    ) -> Result<u8, failure::TestFailure> {
        let located =
            LocatedSource::acquire(store, &0, &self.route(archive)).map_err(failure::acquire)?;
        let location = *located.location();
        claims
            .claim(&self.subject, located, &self.source, Span::none())
            .map_err(failure::conflict)?;
        Ok(location)
    }
}

/// The failure halves a test asserts apart. Which half failed is the fact;
/// the payload underneath it is named by the variant it rode in on.
mod failure {
    use super::*;

    pub(super) enum TestFailure {
        Acquire,
        Conflict,
    }

    pub(super) fn acquire(_: AcquireFailure) -> TestFailure {
        TestFailure::Acquire
    }

    pub(super) fn conflict(_: Failure) -> TestFailure {
        TestFailure::Conflict
    }
}

#[test]
fn a_repeated_request_cannot_hide_adapter_failure() {
    for archive in [false, true] {
        let request = Request::new();
        let store = Locator {
            location: Cell::new(Some(1)),
            calls: Cell::new(0),
        };
        let mut claims = Claims::default();
        assert!(request.locate(&mut claims, &store, archive).is_ok());
        store.location.set(None);
        assert!(
            request.locate(&mut claims, &store, archive).is_err(),
            "a failed locate cannot ride on the claim before it"
        );
        assert_eq!(store.calls.get(), 2);
    }
}

#[test]
fn failed_location_does_not_reserve_a_subject() {
    let request = Request::new();
    let store = Locator {
        location: Cell::new(None),
        calls: Cell::new(0),
    };
    let mut claims = Claims::default();
    assert!(request.locate(&mut claims, &store, false).is_err());
    assert!(claims.0.is_empty());
    store.location.set(Some(2));
    assert!(matches!(request.locate(&mut claims, &store, false), Ok(2)));
}

#[test]
fn conflict_preserves_the_original_claim() {
    let request = Request::new();
    let store = Locator {
        location: Cell::new(Some(1)),
        calls: Cell::new(0),
    };
    let mut claims = Claims::default();
    assert!(request.locate(&mut claims, &store, false).is_ok());
    store.location.set(Some(2));
    assert!(request.locate(&mut claims, &store, false).is_err());
    store.location.set(Some(1));
    assert!(matches!(request.locate(&mut claims, &store, false), Ok(1)));
    assert_eq!(store.calls.get(), 3);
}
