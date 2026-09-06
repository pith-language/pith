use proptest::prelude::*;

use super::*;

proptest! {
    #[test]
    fn arbitrary_failed_attempts_cannot_replace_an_accepted_claim(
        original in any::<u8>(),
        attempts in prop::collection::vec(prop::option::of(any::<u8>()), 0..64),
        archive in any::<bool>(),
    ) {
        let request = Request::new();
        let store = Locator { location: Cell::new(Some(original)), calls: Cell::new(0) };
        let mut claims = Claims::default();
        prop_assert!(matches!(request.locate(&mut claims, &store, archive), Ok(location) if location == original));
        for attempt in &attempts {
            store.location.set(*attempt);
            let result = request.locate(&mut claims, &store, archive);
            match attempt {
                None => prop_assert!(matches!(result, Err(Failure::Acquire(_)))),
                Some(location) if *location == original => prop_assert!(matches!(result, Ok(location) if location == original)),
                Some(_) => prop_assert!(matches!(result, Err(Failure::Conflict(_)))),
            }
        }
        store.location.set(Some(original));
        prop_assert!(matches!(request.locate(&mut claims, &store, archive), Ok(location) if location == original));
        prop_assert_eq!(store.calls.get(), attempts.len().saturating_add(2));
    }
}
