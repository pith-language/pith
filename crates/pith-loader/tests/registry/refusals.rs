//! The checks, each refused at its own boundary, with the rotation
//! and rollback fixtures around them.

use std::collections::BTreeSet;
use std::fs;

use crate::fixture::{
    ADMITTED, TestResult, admitted_with_prior, carries, derive_module, file, opened, opened_ok,
    opened_with_prior, pinned_key, publisher, root, serving_registry, serving_registry_with,
    staged_project, successor,
};
use pith_hir::FrontendCode;
use pith_loader::registry::{Host, Record, RegistryStore};

#[test]
fn a_line_signed_by_a_key_absent_from_the_domain_key_set_refuses() -> TestResult {
    let registry = serving_registry_with(|host, staging| {
        host.enroll(
            &root(),
            "example",
            &BTreeSet::from([publisher().public()]),
            1,
            1,
        )?;
        let dep = derive_module(&staging.join("dep"))?;
        host.publish(&dep, &successor(), ADMITTED)?;
        Ok(())
    })?;
    let diagnostics = match RegistryStore::open(
        registry.path().to_path_buf(),
        &pinned_key(),
        &Record::empty(),
    ) {
        Ok(_) => unreachable!("an unenrolled key's line is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::UnenrolledKey),
        "the refusal names the key and the set: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn a_key_set_not_signed_by_the_pinned_root_refuses_distinctly() -> TestResult {
    let registry = serving_registry_with(|host, staging| {
        host.enroll(
            &successor(),
            "example",
            &BTreeSet::from([publisher().public()]),
            1,
            1,
        )?;
        let dep = derive_module(&staging.join("dep"))?;
        host.publish(&dep, &publisher(), ADMITTED)?;
        Ok(())
    })?;
    let diagnostics = match RegistryStore::open(
        registry.path().to_path_buf(),
        &pinned_key(),
        &Record::empty(),
    ) {
        Ok(_) => unreachable!("a key set the root never signed is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::UnsignedKeySet),
        "the root-mismatch refusal is its own boundary: {diagnostics:?}"
    );
    assert!(
        !carries(&diagnostics, FrontendCode::UnenrolledKey),
        "a bad key-set signature is not a bad release signature: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn a_generation_that_moved_backwards_refuses_as_rollback() -> TestResult {
    let registry = serving_registry()?;
    // Read at generation 1, rotate to generation 2, and read what the
    // rotation served: the consumer's record now holds generation 2.
    let (_store, first) = opened_ok(registry.path())?;
    Host::new(registry.path()).enroll(
        &root(),
        "example",
        &BTreeSet::from([successor().public()]),
        1,
        2,
    )?;
    let (_store, rotated) = admitted_with_prior(registry.path(), &first)?;
    // Replay the generation-1 set the consumer has already moved past.
    Host::new(registry.path()).enroll(
        &root(),
        "example",
        &BTreeSet::from([publisher().public()]),
        1,
        1,
    )?;
    let diagnostics = match opened_with_prior(registry.path(), &rotated) {
        Ok(_) => unreachable!("a generation that moved backwards is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::KeySetRollback),
        "the rollback refusal is its own boundary, not a signature failure: {diagnostics:?}"
    );
    assert!(!carries(&diagnostics, FrontendCode::UnenrolledKey));
    Ok(())
}

#[test]
fn a_previously_admitted_line_now_altered_refuses_as_a_fork() -> TestResult {
    let registry = serving_registry()?;
    let (_store, prior) = opened_ok(registry.path())?;
    // Rewrite the admission time of the dep's line: unsigned by either
    // key, so the publisher's signature still holds and only the
    // append-only check can catch it.
    let dep_index = registry.path().join("index/example/dep");
    let text = fs::read_to_string(&dep_index)?;
    let altered = text.replace(&format!("{ADMITTED}"), &format!("{}", ADMITTED + 1));
    fs::write(&dep_index, altered)?;
    let diagnostics = match opened_with_prior(registry.path(), &prior) {
        Ok(_) => unreachable!("an altered previously admitted line is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::IndexFork),
        "the fork refusal names the line and both states: {diagnostics:?}"
    );
    assert!(
        !carries(&diagnostics, FrontendCode::UnenrolledKey),
        "the admission time is covered by no signature, so only the fork check fires: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn key_rotation_enrolls_the_successor_and_retires_the_old_key() -> TestResult {
    let registry = serving_registry()?;
    let (_store, admitted) = opened_ok(registry.path())?;
    Host::new(registry.path()).enroll(
        &root(),
        "example",
        &BTreeSet::from([successor().public()]),
        1,
        2,
    )?;
    // The lines admitted under generation 1 still read: rotation retires a
    // key for what it may sign next, and the consumer's record already
    // holds what it signed while enrolled.
    admitted_with_prior(registry.path(), &admitted)?;

    // A release signed by the retired key is new to the consumer and
    // answers to the generation-2 set, which does not carry it.
    let staging = tempfile::tempdir()?;
    staged_project(
        staging.path(),
        "module example/other 1.0.0\n",
        "module example/root 0.1.0\n",
    )?;
    let other = derive_module(&staging.path().join("dep"))?;
    Host::new(registry.path()).publish(&other, &publisher(), ADMITTED + 120)?;
    let diagnostics = match opened_with_prior(registry.path(), &admitted) {
        Ok(_) => unreachable!("the retired key's new line is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::UnenrolledKey),
        "the refusal names the retired key and the new set: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn two_entries_for_one_canonical_version_refuse() -> TestResult {
    let registry = serving_registry_with(|host, staging| {
        host.enroll(
            &root(),
            "example",
            &BTreeSet::from([publisher().public()]),
            1,
            1,
        )?;
        let dep = derive_module(&staging.join("dep"))?;
        host.publish(&dep, &publisher(), ADMITTED)?;
        // The same release under a spelling that compares equal: `1.2`
        // against `1.2.0`, so a person reads one version where the index
        // would carry two.
        file(&staging.join("dep/module.pi"), "module example/dep 1.2\n")?;
        let respelled = derive_module(&staging.join("dep"))?;
        host.publish(&respelled, &publisher(), ADMITTED + 30)?;
        Ok(())
    })?;
    let diagnostics = match opened(registry.path()) {
        Ok(_) => unreachable!("two entries for one canonical version are refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::DuplicateVersion),
        "versions that compare equal are one version: {diagnostics:?}"
    );
    Ok(())
}
