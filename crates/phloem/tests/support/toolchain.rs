use xylem::{DiscoveryError, Toolchain};

pub fn toolchain_or_skip(driver: &str) -> Result<Option<Toolchain>, String> {
    match Toolchain::discover(driver) {
        Ok(toolchain) => Ok(Some(toolchain)),
        Err(DiscoveryError::NotFound) => {
            eprintln!("skipping: no {driver} driver on this host");
            Ok(None)
        }
        Err(error) => Err(format!("{driver} is present but discovery failed: {error}")),
    }
}

pub fn assert_c_toolchain_available(context: &str) {
    let outcomes: Vec<(&str, Result<Toolchain, DiscoveryError>)> = ["cc", "gcc", "clang"]
        .into_iter()
        .map(|driver| (driver, Toolchain::discover(driver)))
        .collect();
    for (driver, outcome) in &outcomes {
        if let Err(error) = outcome {
            assert!(
                matches!(error, DiscoveryError::NotFound),
                "{driver} is present but discovery failed: {error}"
            );
        }
    }
    assert!(
        outcomes.iter().any(|(_, outcome)| outcome.is_ok()),
        "no C compiler (cc, gcc, or clang) on this host: {context}: {outcomes:?}"
    );
}
