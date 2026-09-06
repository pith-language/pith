use std::cell::Cell;

use super::*;
use crate::{AcquiredManifest, AcquiredSource};
use pith_diag::SourceId;

struct Locator {
    location: Cell<Option<u8>>,
    calls: Cell<usize>,
}

impl ModuleStore for Locator {
    type Location = u8;

    fn locate(&self, _: &u8, _: &Route<'_>) -> Result<u8, AcquireFailure> {
        self.calls.set(self.calls.get().saturating_add(1));
        self.location
            .get()
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: "reference refused".into(),
            })
    }

    fn manifest(&mut self, _: &u8) -> Result<AcquiredManifest, AcquireFailure> {
        unreachable!("claim agreement does not acquire manifest bytes")
    }

    fn sources(&mut self, _: &u8) -> Result<Vec<AcquiredSource>, AcquireFailure> {
        unreachable!("claim agreement does not acquire source bytes")
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
            source: Arc::new(SourceFile::new(SourceId::from_raw(0), "module.pi", "")),
        }
    }

    fn route(&self, archive: bool) -> Route<'_> {
        if archive {
            Route::Archive {
                subject: &self.subject,
                url: "archive",
                digest: "unvalidated",
            }
        } else {
            Route::Git {
                subject: &self.subject,
                url: "repository",
                revision: "main",
                subpath: None,
            }
        }
    }

    fn locate(
        &self,
        claims: &mut Claims<u8>,
        store: &Locator,
        archive: bool,
    ) -> Result<u8, Failure> {
        claims.locate(
            store,
            &0,
            &self.route(archive),
            &self.subject,
            &self.source,
            Span::none(),
        )
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
        assert!(matches!(
            request.locate(&mut claims, &store, archive),
            Err(Failure::Acquire(_))
        ));
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
    assert!(matches!(
        request.locate(&mut claims, &store, false),
        Err(Failure::Conflict(_))
    ));
    store.location.set(Some(1));
    assert!(matches!(request.locate(&mut claims, &store, false), Ok(1)));
    assert_eq!(store.calls.get(), 3);
}
