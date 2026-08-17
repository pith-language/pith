cargo-nightly := env("CARGO_NIGHTLY", "cargo +nightly")

# Print the available recipes (default when no recipe is given).
[default]
[private]
list:
    @just --list --unsorted

# Enter the nix devshell (for environments without direnv).
[group('setup')]
dev:
    nix develop

# Print the toolchain versions the devshell resolved to.
[group('setup')]
versions:
    rustc --version
    cargo --version
    cargo nextest --version
    cargo mutants --version

# Format every crate.
[group('lint')]
fmt:
    cargo fmt --all

# Check formatting without writing.
[group('lint')]
fmt-check:
    cargo fmt --all -- --check

# Run clippy across the workspace with the project's lint posture.
[group('lint')]
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# A fast combined check: format + clippy. No tests.
[group('lint')]
check: fmt-check clippy

# Run the unit/integration test suite via nextest.
[group('test')]
test:
    cargo nextest run --workspace

# Run doctests (nextest does not run doctests, so use cargo).
[group('test')]
test-doc:
    cargo test --workspace --doc

# Run a single test by name substring (e.g. `just test-one cycle`).
[group('test')]
test-one name:
    cargo nextest run --workspace {{ name }}

# Watch for changes and re-run the fast check + tests. Uses bacon.
[group('test')]
watch:
    bacon check

# Watch and re-run clippy only.
[group('lint')]
watch-clippy:
    bacon clippy

# Everything CI checks, in one target. Use before pushing.
[group('ci')]
ci: fmt-check clippy test test-doc docs-check deny typos determinism just-fmt-check

# The xtask determinism guard (no HashMap in source, decision 0021).
[group('ci')]
determinism:
    cargo run -p xtask -- check-determinism

# Build the workspace documentation and fail on rustdoc warnings/lints.
[group('ci')]
docs:
    cargo doc --workspace --no-deps --document-private-items

# Check docs without leaving build artifacts (what CI does).
[group('ci')]
docs-check:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items

# cargo-deny: licenses, advisories, bans, sources.
[group('ci')]
deny:
    cargo deny check

# Spell-check the whole tree.
[group('ci')]
typos:
    typos

# Check that this justfile is canonically formatted.
[group('ci')]
just-fmt-check:
    just --fmt --check

# Run tests across every feature combination (catches feature-gate bugs).
[group('matrices')]
hack-features:
    cargo hack --workspace --feature-powerset --exclude-features default --no-dev-deps nextest run

# Run tests with --no-default-features per crate.
[group('matrices')]
hack-no-default:
    cargo hack --workspace --each-feature --no-dev-deps nextest run

# Measure pure evaluation at graph sizes the test suite does not reach.
[group('test')]
bench:
    cargo bench -p pith-engine

# Generate a coverage report (HTML + text summary) into target/llvm-cov/html.
[group('coverage')]
cov:
    cargo llvm-cov nextest --workspace --html

# Open the HTML coverage report (Linux).
[group('coverage')]
[linux]
cov-open: cov
    xdg-open target/llvm-cov/html/index.html

# Open the HTML coverage report (macOS).
[group('coverage')]
[macos]
cov-open: cov
    open target/llvm-cov/html/index.html

# Run cargo-mutants across the whole workspace. Output lands in mutants.out/.
[group('mutants')]
mutants:
    cargo mutants --workspace --output mutants.out

# Run mutants against a single crate (faster iteration).
[group('mutants')]
mutants-crate crate:
    cargo mutants -p {{ crate }} --output mutants.out

# Run mutants against specific files only.
[group('mutants')]
mutants-files +files:
    cargo mutants --workspace --file {{ files }} --output mutants.out

# Summarize the last mutants run (missed/caught/unviable/timeout).
[group('mutants')]
[private]
mutants-summary:
    @echo "Missed (surviving) mutants:"
    @jq -r '.mutations[] | select(.summary=="missed") | "  " + .source + ":" + (.line|tostring) + ":" + (.col|tostring) + " " + .function' mutants.out/mutants.json 2>/dev/null || echo "  (no mutants.out/mutants.json found; run `just mutants` first)"
    @echo ""
    @echo "Timeouts (possible hangs):"
    @jq -r '.mutations[] | select(.summary=="timeout") | "  " + .source + ":" + (.line|tostring) + ":" + (.col|tostring) + " " + .function' mutants.out/mutants.json 2>/dev/null || true

# The codec property tests. Named fuzz_* for the contracts they cover, but they
# are proptest suites `just test` already runs, not cargo-fuzz targets: there is
# no fuzz/ directory and no libfuzzer harness in the tree.
[group('test')]
test-codec-properties:
    cargo nextest run -p pith-core --test fuzz_codec --test fuzz_action_spec

# Run Miri on the safe crates. It cannot run the FFI in pith-executor-local
# (landlock/seccomp); scope it to crates with no `unsafe`.
[group('miri')]
miri:
    {{ cargo-nightly }} miri test -p pith-core -p pith-ids -p pith-arena -p pith-diag -p pith-output -p pith-store

# Find unused dependencies in Cargo.toml files.
[group('hygiene')]
machete:
    cargo machete

# Lint nix files for dead bindings.
[group('hygiene')]
nix-lint:
    deadnix --fail flake.nix nix/
    statix check flake.nix nix/

# Check documentation links (the docs/ notebook has many cross-references).
[group('hygiene')]
links:
    lychee --no-progress docs/ README.md

# Clean all build artifacts.
[confirm]
[group('convenience')]
clean:
    cargo clean

# Update Cargo.lock and the flake inputs.
[group('convenience')]
update:
    cargo update
    nix flake update
