# pith task runner. Run `just` (or `just --list`) to see all recipes.
#
# This assumes the nix devshell is active (`nix develop` or direnv). All tools
# are provided by nix/devshell.nix so there is nothing to install by hand.
#
# Recipes are grouped: fast-feedback checks first, then the full CI mirror,
# then heavier investigative tools (mutants, fuzz, miri, coverage).

default:
    @just --list --unsorted

# ---------------------------------------------------------------------------
# setup
# ---------------------------------------------------------------------------

# Enter the nix devshell (for environments without direnv).
dev:
    nix develop

# Print the toolchain versions the devshell resolved to.
versions:
    rustc --version
    cargo --version
    cargo nextest --version
    cargo mutants --version

# ---------------------------------------------------------------------------
# fast feedback (the loop you run while coding)
# ---------------------------------------------------------------------------

# Format every crate.
fmt:
    cargo fmt --all

# Check formatting without writing.
fmt-check:
    cargo fmt --all -- --check

# Run clippy across the workspace with the project's lint posture.
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# A fast combined check: format + clippy. No tests.
check: fmt-check clippy

# Run the unit/integration test suite via nextest.
test:
    cargo nextest run --workspace

# Run tests including doctests (nextest does not run doctests, so use cargo).
test-doc:
    cargo test --workspace --doc

# Run a single test by name substring (e.g. `just test-one cycle`).
test-one name:
    cargo nextest run --workspace {{name}}

# Watch for changes and re-run the fast check + tests. Uses bacon.
watch:
    bacon check

# Watch and re-run clippy only.
watch-clippy:
    bacon clippy

# ---------------------------------------------------------------------------
# ci mirror (what .github/workflows/ci.yml runs, runnable locally)
# ---------------------------------------------------------------------------

# Everything CI checks, in one target. Use before pushing.
ci: fmt-check clippy test test-doc docs-check deny typos determinism

# The xtask determinism guard (no HashMap in source, decision 0021).
determinism:
    cargo run -p xtask -- check-determinism

# Build the workspace documentation and fail on rustdoc warnings/lints.
docs:
    cargo doc --workspace --no-deps --document-private-items

# Check docs without leaving build artifacts (what CI does).
docs-check:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items

# cargo-deny: licenses, advisories, bans, sources.
deny:
    cargo deny check

# Spell-check the whole tree.
typos:
    typos

# ---------------------------------------------------------------------------
# feature and target matrices
# ---------------------------------------------------------------------------

# Run tests across every feature combination (catches feature-gate bugs).
hack-features:
    cargo hack --workspace --feature-powerset --exclude-features default --no-dev-deps nextest run

# Run tests with --no-default-features per crate.
hack-no-default:
    cargo hack --workspace --each-feature --no-dev-deps nextest run

# ---------------------------------------------------------------------------
# coverage
# ---------------------------------------------------------------------------

# Generate a coverage report (HTML + text summary) into target/llvm-cov.
cov:
    cargo llvm-cov --workspace --nextest --html

# Open the HTML coverage report.
cov-open: cov
    xdg-open target/llvm-cov/index.html 2>/dev/null || open target/llvm-cov/index.html

# ---------------------------------------------------------------------------
# mutation testing (investigative; slow)
# ---------------------------------------------------------------------------

# Run cargo-mutants across the whole workspace. Output lands in mutants.out/.
mutants:
    cargo mutants --workspace --output mutants.out

# Run mutants against a single crate (faster iteration).
mutants-crate crate:
    cargo mutants -p {{crate}} --output mutants.out

# Run mutants against specific files only.
mutants-files +files:
    cargo mutants --workspace --file {{files}} --output mutants.out

# Summarize the last mutants run (missed/caught/unviable/timeout).
mutants-summary:
    @echo "Missed (surviving) mutants:"
    @jq -r '.mutations[] | select(.summary=="missed") | "  " + .source + ":" + (.line|tostring) + ":" + (.col|tostring) + " " + .function' mutants.out/mutants.json 2>/dev/null || echo "  (no mutants.out/mutants.json found; run `just mutants` first)"
    @echo ""
    @echo "Timeouts (possible hangs):"
    @jq -r '.mutations[] | select(.summary=="timeout") | "  " + .source + ":" + (.line|tostring) + ":" + (.col|tostring) + " " + .function' mutants.out/mutants.json 2>/dev/null || true

# ---------------------------------------------------------------------------
# fuzzing (nightly; investigative)
# ---------------------------------------------------------------------------

# NOTE: cargo-fuzz needs a nightly toolchain. The devshell does not pin one by
# default. Install it with: `nix run nixpkgs#rustup -- toolchain install
# nightly --component rust-src && nix run nixpkgs#rustup -- run nightly cargo
# install cargo-fuzz`, then use `just fuzz-<target>`.

# Run the codec round-trip / no-panic fuzzer.
fuzz-codec:
    cargo +nightly fuzz run fuzz_codec -- -max_total_time=120

# Run the action-spec digest fuzzer.
fuzz-action-spec:
    cargo +nightly fuzz run fuzz_action_spec -- -max_total_time=120

# ---------------------------------------------------------------------------
# miri (nightly; investigative)
# ---------------------------------------------------------------------------

# NOTE: Miri needs a nightly toolchain (see `fuzz` note above) plus the miri
# component: `nix run nixpkgs#rustup -- component add miri --toolchain nightly`.
# It cannot run the FFI in pith-executor-local (landlock/seccomp); scope it to
# the safe crates.

miri:
    cargo +nightly miri test -p pith-core -p pith-ids -p pith-arena -p pith-diag -p pith-output -p pith-store

# ---------------------------------------------------------------------------
# hygiene
# ---------------------------------------------------------------------------

# Find unused dependencies in Cargo.toml files.
machete:
    cargo machete

# Lint nix files for dead bindings.
nix-lint:
    deadnix --fail flake.nix nix/
    statix check flake.nix nix/

# Check documentation links (the docs/ notebook has many cross-references).
links:
    lychee --no-progress docs/ README.md

# ---------------------------------------------------------------------------
# convenience
# ---------------------------------------------------------------------------

# Clean all build artifacts except the cargo registry cache.
clean:
    cargo clean

# Update Cargo.lock and the flake inputs.
update:
    cargo update
    nix flake update
