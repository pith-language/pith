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

#[path = "support/diagnostic.rs"]
mod diagnostic_support;
#[path = "support/toolchain.rs"]
mod toolchain_support;

use diagnostic_support::fixture_error;
use pith_core::Value;
use pith_engine::{AllowAllActions, Engine, Evaluation, TokioRuntime};
use pith_executor_local::LocalExecutor;
use pith_ids::ContentId;
use pith_store::{ContentStore, FilesystemContentStore};
use toolchain_support::{assert_c_toolchain_available, toolchain_or_skip};
use xylem::{BuildEngine, HeaderUniverse, Toolchains, types};

const SOURCE: &[u8] = b"int main(void) { return 0; }\n";
const ELF_MAGIC: &[u8] = b"\x7fELF";

/// A host with no C compiler at all cannot run this file's other test, and a
/// green run over skips would read as a verified base case. Fail instead.
#[test]
fn a_c_toolchain_is_available() {
    assert_c_toolchain_available("the base case cannot run, and its other test would skip green");
}

#[test]
fn an_executable_builds_with_no_package_defined_anywhere() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let store = FilesystemContentStore::open(root.path()).unwrap();
    let mut engine = Engine::with_content_store(store);
    engine.register_xylem(Toolchains::one(toolchain.clone()), HeaderUniverse::empty());
    let source = engine.put_blob(SOURCE).unwrap();

    let compile = types::compile_request(
        toolchain.value(),
        source,
        xylem::types::provided_headers([] as [(Box<str>, ContentId); 0]),
    );
    let evaluation = run(&mut engine, &compile).unwrap();
    let object = blob_of(&evaluation.value).unwrap();

    let link = types::link_request(toolchain.value(), [object]);
    let evaluation = run(&mut engine, &link).unwrap();
    let executable = blob_of(&evaluation.value).unwrap();

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
    make_executable(&program).unwrap();
    let status = std::process::Command::new(&program).status().unwrap();
    assert!(
        status.success(),
        "the built executable exited with {status}"
    );
}

/// The fixture's own failure spelling, as `source_adapter.rs` states it: a
/// helper outside a `#[test]` function cannot unwrap or panic under the
/// crate's lint posture, so failures travel as diagnostics.
fn run(
    engine: &mut Engine,
    request: &pith_core::Request<pith_core::Pure>,
) -> pith_diag::PithResult<Evaluation> {
    let runtime = TokioRuntime::new()
        .map_err(|error| fixture_error(format!("constructing the runtime failed: {error:?}")))?;
    engine
        .run(request, &runtime, &AllowAllActions, &LocalExecutor::new())
        .map_err(|error| fixture_error(format!("the runtime could not drive the run: {error:?}")))?
}

fn blob_of(value: &Value) -> pith_diag::PithResult<ContentId> {
    match value {
        Value::Nominal { representation, .. } => match representation.as_ref() {
            Value::Blob(id) => Ok(*id),
            _ => Err(fixture_error(
                "a nominal content value carried no blob".into(),
            )),
        },
        _ => Err(fixture_error(
            "the value was not a nominal content value".into(),
        )),
    }
}

fn make_executable(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}
