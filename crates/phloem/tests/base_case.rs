//! Scope's base case, held unchanged (0009, 0039): someone builds one
//! executable without defining a package, and this file builds one with no
//! phloem value constructed anywhere — the compile and link requests are
//! xylem's own, driven through the kernel exactly as a package-less build
//! drives them. The phloem crate appears only as the test's host, which is
//! the point: packaging is a peer, not a parent.
//!
//! Linux-gated like xylem's toolchain tests, with the same
//! `toolchain_or_skip` discipline: skip only on genuine absence, fail on a
//! driver that is present but undiscoverable, and fail the run outright when
//! no compiler exists at all so a compiler-less host cannot read as green.

#![cfg(target_os = "linux")]

use pith_core::Value;
use pith_engine::{AllowAllActions, Engine, Evaluation, TokioRuntime};
use pith_executor_local::LocalExecutor;
use pith_ids::ContentId;
use pith_store::{ContentStore, FilesystemContentStore};
use xylem::{BuildEngine, DiscoveryError, HeaderUniverse, Toolchain, Toolchains, types};

const SOURCE: &[u8] = b"int main(void) { return 0; }\n";
const ELF_MAGIC: &[u8] = b"\x7fELF";

fn toolchain_or_skip(driver: &str) -> Option<Toolchain> {
    match Toolchain::discover(driver) {
        Ok(toolchain) => Some(toolchain),
        Err(DiscoveryError::NotFound) => {
            eprintln!("skipping: no {driver} driver on this host");
            None
        }
        // Reachable by design: the driver is on the host but could not be
        // resolved into a closure, which is a failure to report rather than
        // an absence to skip on.
        Err(error) => panic!("{driver} is present but discovery failed: {error}"),
    }
}

/// A host with no C compiler at all cannot run this file's other test, and a
/// green run over skips would read as a verified base case. Fail instead.
#[test]
fn a_c_toolchain_is_available() {
    for driver in ["cc", "gcc", "clang"] {
        match Toolchain::discover(driver) {
            Ok(_) => return,
            Err(DiscoveryError::NotFound) => {}
            Err(error) => panic!("{driver} is present but discovery failed: {error}"),
        }
    }
    panic!("no C compiler (cc, gcc, or clang) on this host: the base case cannot run");
}

#[test]
fn an_executable_builds_with_no_package_defined_anywhere() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let store = FilesystemContentStore::open(root.path()).unwrap();
    let mut engine = Engine::with_content_store(store);
    engine.register_xylem(Toolchains::one(toolchain.clone()), HeaderUniverse::empty());
    let source = engine.put_blob(SOURCE).unwrap();

    let compile = types::compile_request(toolchain.value(), source);
    let evaluation = run(&mut engine, &compile);
    let object = blob_of(&evaluation.value);

    let link = types::link_request(toolchain.value(), [object]);
    let evaluation = run(&mut engine, &link);
    let executable = blob_of(&evaluation.value);

    let store = FilesystemContentStore::open(root.path()).unwrap();
    let bytes = store.get_blob(executable).unwrap().unwrap();
    assert!(
        bytes.as_bytes().starts_with(ELF_MAGIC),
        "the package-less build produced a linked executable"
    );

    // The built program runs and reports success: the executable works, not
    // just bytes with the right magic.
    let program = root.path().join("program");
    std::fs::write(&program, bytes.as_bytes()).unwrap();
    make_executable(&program);
    let status = std::process::Command::new(&program).status().unwrap();
    assert!(
        status.success(),
        "the built executable exited with {status}"
    );
}

fn run(engine: &mut Engine, request: &pith_core::Request<pith_core::Pure>) -> Evaluation {
    let runtime = TokioRuntime::new().unwrap();
    match engine.run(request, &runtime, &AllowAllActions, &LocalExecutor::new()) {
        Ok(Ok(evaluation)) => evaluation,
        Ok(Err(diagnostics)) => panic!("the build failed: {diagnostics:?}"),
        Err(error) => panic!("the runtime could not drive the run: {error:?}"),
    }
}

fn blob_of(value: &Value) -> ContentId {
    match value {
        Value::Nominal { representation, .. } => match representation.as_ref() {
            Value::Blob(id) => *id,
            _ => panic!("a nominal content value carried no blob"),
        },
        _ => panic!("the value was not a nominal content value"),
    }
}

fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
