---
schema: design-doc/v1
id: research-wasm-components
title: wasm components as host code
summary: the WebAssembly component model and Wasmtime as of september 2026, eleven systems that run third-party wasm, how compiled wasm is distributed, and what that means for domains whose declarations are pith and whose host bodies are components
kind: research
status: researching
evidence: preliminary
created: 2026-09-29
updated: 2026-09-29
tags:
  - research
  - libraries
  - extensibility
  - security
  - wasm
relations:
  informed_by:
    - research-extension-interfaces
    - research-module-distribution
    - research-registry-supply-chain-security
  depends_on:
    - research-method
  supersedes: []
---

# wasm components as host code

a module written outside this repository can declare types and rule interfaces in `.pi`, and its
represented pure bodies run. its `= host` declarations do not. a host body is rust linked into the
binary, the cli links no domain crate, and `bind_module` in `crates/pith-query/src/program.rs` binds
every host rule to a diagnostic saying so.

this note asks whether those bodies could ship as WebAssembly components: loaded at run time, typed by
the `.pi` declarations, deterministic enough to cache by digest, bounded, and safe to run when the author
is unknown.

everything below was read on 2026-09-29. the component model changes monthly, and several of these facts
are a few weeks old.

## the standard

WASI 0.2 shipped in january 2024 and reached 0.2.12 on 2026-06-02. WASI 0.3.0 followed on 2026-06-11
after a vote in the WASI subgroup. the Bytecode Alliance's announcement calls it "a stable release,
which means programs you compile for it today are guaranteed to keep working in the future." 0.3 adds
`async func`, `stream<T>` and `future<T>`, and removes `wasi:io`, whose job moves into canonical ABI
built-ins. 0.3.1 followed on 2026-08-11 with a `map<K, V>` type and the `implements` and `external-id`
annotations. point releases now come every two months.

the stability promise comes from the Bytecode Alliance and the subgroup. the component model is still a
phase 1 proposal in the W3C community group's table (last changed 2026-08-10), the readme still calls
the WASI releases "Developer Preview", and WASI 1.0 has no date. per the explainer, features
that have shipped in a WASI release "are not open to breaking changes before the final 1.0 release", and
everything else "may have breaking changes".

nothing has been announced for 0.3.2. a feature reaches a release only after a subgroup vote; WASI issues
#942 and #943 show that process for the two features 0.3.1 took. the features the explainer lists as
not yet shipped are:

- value imports and exports, and a component-level start function
- nested namespaces and packages in import and export names
- more canonical ABI options on async built-ins
- async lift without a callback (stackful lift)
- threading built-ins, and a later variant built on shared-everything threads
- fixed-length lists
- the `error-context` type
- canonical interface names
- memory64
- getters and setters
- `stream.forward` and `future.forward`

## what WIT can carry

WIT's value types are integers up to `u64` and `s64`, `f32` and `f64`, `bool`, `char`, `string`,
records, variants, enums, flags, `option`, `result`, tuples, `list<T>`, `map<K, V>` with keys limited to
booleans, integers, `char` and `string`, `stream<T>`, `future<T>`, and owned or borrowed handles to
resources.

three of pith's types have no direct counterpart.

recursive types are not allowed. WIT.md: "Types, however, cannot be recursive." the explainer says the
type grammar has nothing like wasm-gc's `rectype`. issue #56 has asked for recursion since 2022; in april
2025 Luke Wagner called it "pretty tricky" and suggested following wasm-gc. nothing has been specified.
pith's `Type::Cut` lets a declaration refer directly to itself, so `Tree = Node(List<Tree>) | Leaf(Int)`
is valid pith and has no WIT spelling.

there is no unbounded integer. [0055](../decisions/0055-arbitrary-precision-int.md) made pith's `Int`
arbitrary-precision, and the widest WIT integer is 64 bits.

type identity is structural. the explainer: "all type constructors are *structural* with the exception of
`resource`, which is *abstract* and *generative*." a record has to be exported under a name so a bindings
generator can refer to it, "even though these types are purely structural types". field and case labels
are part of a type; the record's own name is not. a pith `nominal Message = Text` and a plain `Text` are
the same value once they cross. subtyping is relaxed only for component, instance and module types, so
adding a case to an exported variant breaks consumers.

## the canonical ABI

values cross by copy. lifting reads a value out of one component's linear memory, lowering writes it
into the other's through the callee's `realloc`, and records and lists are walked element by element.
each request and answer costs at least one copy of every string and list in each direction. a content
identity is 32 bytes whatever the size of the blob it names.

value definitions come closest to a specified value encoding: Binary.md gives each type a byte form
(LEB128 integers, length-prefixed lists, a `u32` discriminant for variants, a fixed NaN pattern). they
are not shipped, and the form is not canonical. LEB128 allows padded encodings of one number and nothing
orders map entries, so a digest over it would give one value several identities.

## resources, re-entrance and traps

an instance's handle table and linear memory persist across calls, so a resource can keep state behind
its handle until the owning handle is dropped. a `resource frame` with a `step` method would be a direct
translation of `PureRuleFrame::step` in `crates/pith-engine/src/graph/ir.rs`, using only 0.2 features.

two changes landed in the last month. PR #705, merged 2026-08-28, removed the `may_enter` flag
and its trap and added tests for re-entrant calls "that used to trap but are now valid". the concurrency
document now tells hosts to re-enter a component only "when explicitly supported by the component's
documented API", so a host that must not re-enter a guest mid-step has to enforce it. PR #728, merged
2026-09-28, specified a lockdown state set on trap, after which "it's no longer possible to observe the
internal state of a component instance". a trap ends its store.

## determinism

the WebAssembly 3.0 core specification defines a deterministic profile. under it, "All NaN values
generated by floating-point instructions are canonical and positive," and "All relaxed vector
instructions have a fixed behaviour that does not depend on the implementation." memory and table growth
stay nondeterministic "in order to be able to indicate resource exhaustion." the design repository's
Nondeterminism.md adds shared-memory threads, stack exhaustion, and whatever the host's imports return.
the component model's concurrency document lists its own scheduling choices and says "each of these
sources of nondeterminism can be removed by a host implementing the WebAssembly Deterministic Profile."
it describes `error-context` as "immutable, nondeterministic, host-defined" and says components "*must
not* depend on" its contents.

so a body's output depends only on its input if the host canonicalizes NaNs, fixes or disables relaxed
SIMD, forbids shared memory, provides only imports whose answers are deterministic, and treats running
out of any bound as a failure with no value.

## the runtime

Wasmtime is the only rust host that runs components in production. wasmi is deterministic and meters fuel
but runs core modules only (issue #897 is open). Wasmer closed its WASI preview 2 issue as not planned.
WAMR has no component support and announced on 2026-07-21 that it is leaving the Bytecode Alliance.

Wasmtime ships a major release on the 20th of each month. every twelfth version is a long-term release
supported for 24 months, and two are supported at a time; 36 and 48 are the current pair, and 49.0.1
shipped on 2026-09-24.

### tiers and the security policy

the security policy begins: bugs "must affect a tier 1 platform or feature to be considered a security
vulnerability." tier 1 includes x86_64 linux, Cranelift, Winch, the `component-model` proposal, relaxed
SIMD, GC and exception handling. aarch64 linux is tier 2 for lack of continuous fuzzing, and the Pulley
interpreter is tier 2 pending "more time fuzzing/baking". component-model async, streams, futures and
WASI 0.3 are not listed in any tier, although Wasmtime 46 turned them on by default in june.

under the same policy, a sandbox escape, an out-of-bounds access, or "use of a
WASI resource without having been given the associated WASI capability" is a vulnerability. "Execution
that diverges from Wasm semantics (such as computing incorrect values) are not considered security
vulnerabilities so long as they remain confined within the sandbox." embedders "should never blindly
trust values from the guest ... even if it was written by the embedders themselves". a denial of service
at execution time, including "uninterruptible infinite loops" under fuel and "user-controlled memory
exhaustion", counts as a vulnerability; one at compile time does not.

### recent advisories

on 2026-04-09 Wasmtime published twelve advisories, eleven of them "discovered via LLM tools" over three
weeks. they included an aarch64 Cranelift miscompile of `load(iadd(base, ishl(index, amt)))` allowing
arbitrary host reads and writes (CVE-2026-34971, versions 32 to 43, blocked in practice by the default
spectre mitigations) and a sandbox escape in Winch. the project said it would run LLM-assisted scanning
and aarch64 fuzzing continuously, and move its formal verification of lowering rules into CI. on
2026-09-24 it fixed GHSA-m63x-6p34-q65x, where `call_ref` and exception `catch` could discard spent fuel
and let a guest run far past its budget. Crocus, an SMT-based checker for Cranelift's ISLE lowering rules
published at ASPLOS 2024, reproduced the 2023 x86_64 escape rated 9.9 and found two new bugs.

### instantiation

`Linker::instantiate_pre` resolves and type-checks a component's imports once, and
`InstancePre::instantiate` then only allocates and initializes. with the pooling allocator and
copy-on-write memory images, the 2022 performance post measured SpiderMonkey's instantiation dropping
"from about 2 milliseconds to 5 microseconds". that figure is for one core module. a component
instantiates several core modules and adapters, and no component figure has been published. the store
documentation describes a `Store` as "intended to be a short-lived object", which fits one store per
computation; the cost for pith has to be measured.

### bounds

the `epoch_interruption` documentation separates the two mechanisms: "Fuel, in contrast, should be used
when *deterministic* yielding or trapping is needed ... the same function call with the same starting
state will always either complete or trap with an out-of-fuel error, deterministically." epochs are
faster ("up to 2-3x" in some measurements) and driven by a timer.

fuel is deterministic within one engine version, not across versions. version 46 started charging bulk
memory operations by size; version 49 moved that charge after the operation and applied
`operator_cost` to constant expressions and start functions. the same body, input and budget can finish
under one release and run out under the next.

other bounds are per store: `StoreLimitsBuilder` for linear memory, table elements and instance counts;
`max_wasm_stack`, 512 KiB by default; `Store::set_hostcall_fuel`, which caps how much a guest can make
the host copy in one call; and `ResourceTable::set_max_capacity`. the last two were added for
CVE-2026-27204.

### compiled code

`Component::serialize` writes a precompiled artifact. `Component::deserialize` is `unsafe`: the artifact
is "only lightly validated" and a crafted one "can trivially be used to execute arbitrary code".
`Engine::precompile_compatibility_hash` matches between two engines exactly when artifacts from one load
in the other. since version 38 artifacts stay loadable across patch releases of a major version.

### suspension

async calls run on a separate native stack. when an async host import returns a pending future, Wasmtime
switches back to the caller and reports pending; the guest continues from the same point when the future
completes. a guest can call `need(request)` and wait. the suspended call holds its store until it
finishes, and it cannot be serialized.

## systems that already do this

eleven systems below run third-party wasm as an extension mechanism, grouped by project.

### Zed

Zed tried javascript first. by early 2024, its developers wrote, the component model "had only recently
become usable", and the extension API moved to WIT on `wasm32-wasip2`. users never compile: "You download
and unpack an archive that contains Wasm and Scheme code."

each published API version is a frozen WIT directory under `crates/extension_api/wit/`, nine of them from
`since_v0.0.1` to `since_v0.8.0`. the host has one enum variant per version, chooses one from the version
in the extension's manifest, and converts results from older versions into current types. stable builds
allow a lower maximum version than nightly builds, so a new version can be tried before release.

capabilities came later. a manifest-declared design landed on 2024-08-27 and was reverted the next day.
process capabilities with argument patterns landed in march 2025, and a user-side grant setting in
october 2025, about eighteen months after launch. the documentation warns that restricting grants "will
likely make many extensions non-functional".

### Typst

Typst plugins, added in 0.8.0 in september 2023, have a rule close to pith's: "A plugin function call
must not have any observable side effects on future plugin calls and given the same arguments, it must
always return the same value." plugins are core modules run on wasmi with exactly two imports, both
protocol functions; a module importing anything else fails to link, which keeps WASI out. relaxed SIMD is
disabled with the comment "Disable relaxed SIMD as it can introduce non-determinism." calls are memoized,
and a plugin's identity is its bytes plus a fingerprint of the state transitions applied to it.

pooled instances are reused without resetting memory, so a plugin that keeps state from one call to the
next breaks the rule and nothing detects it. there is no fuel limit.

### Shopify Functions

Shopify runs partner code inside its own request path. its 2020 engineering post gives the reason for
wasm: "You cannot express anything malicious in Wasm. You can only express manipulations of the virtual
environment and use provided imports." functions may not use clocks or randomness, and the limits are
published: 256 kB of binary, 10,000 kB of memory, 11 million instructions, 128 kB of input and 20 kB of
output. the ABI is versioned by import module name, `shopify_function_v1` and then `v2`.

for network access, a `fetch` target returns a description of an HTTP request, Shopify performs it, and
the response is passed to the `run` target as input. the function itself does no I/O. pith separates an
action rule's `plan` from the executor that runs the plan in the same way.

the instruction limit is the most frequent complaint in Shopify's own forums, usually from functions
that parse large carts.

### proxy-wasm

proxy-wasm is a hand-written C-style ABI. a module declares its version by exporting a function that is
never called, `proxy_abi_version_0_2_1`, and host memory is requested through an exported allocator. a
2023 review by Solo.io describes the result: "If the ABI is still in flux, especially between versions
of Envoy, the SDK is limited to whatever version of the ABI it implements." the next version is "very
different from the existing one". wasi-http was later based on it.

### Extism and Helm

Extism uses core modules and a bytes-in, bytes-out ABI so that any language its users write can target
it; in 2023 its maintainer said it would stay that way until the component model supported more
languages. its manifest pins a sha256 of each module and lists the hosts and paths a plugin may use. Helm
4 chose Extism for plugins because WASI 0.2 was "not yet supported by Extism's underlying Go tooling,
Wazero", with allowed hosts defaulting to none.

### Spin and wasmCloud

Spin's manifest denies by default: outbound hosts, files, key-value stores and variables are granted per
component. Spin 3.0 added dependencies on other components, from a registry, a path, or a URL with a
required sha256 digest. a dependency gets no resources unless `dependencies_inherit_configuration` is set,
and then every dependency gets all of them.

wasmCloud 2.0, released in march 2026, removed the out-of-process capability providers of version 1,
citing network overhead, separate deployment, "operational friction" and maintainer time. in-process host
plugins implementing WASI interfaces replaced them.

### rust procedural macros

dtolnay's watt ran procedural macros compiled to wasm in 2019, so that a macro's "only possible
interaction with the world is limited to consuming tokens and producing tokens". compiler MCP #1017,
opened 2026-07-19 and accepted 2026-08-02, adopts wasm macros to "Enable sound(er) caching of proc
macros", describes rustc as acting "as a ~kernel for the running program", and cites HashMap iteration
order lowering cache hit rates in Buck. one of its non-goals is "Make any security guarantees", and it was
seconded on that condition.

there are two open drafts. #157590 uses wasip2, Wasmtime and a WIT file, and runs into `Span` being
`Copy`, which a resource handle cannot be; it considers integer handles instead. #157709 uses a core
module on wasmi and argues that the component model adds a second ABI to maintain and a bootstrap cycle
through wasi-sdk.

### Bazel and proto

Bazel 8.3.0 added `load_wasm` and `execute_wasm` to repository and module contexts behind an experimental
flag, so that one prebuilt blob would behave "with identical behavior and full sandboxing" on every host.
a 2025 issue measured about three seconds for a TOML-to-JSON conversion that took 24 milliseconds under
wazero.

moonrepo's proto added Extism plugins in july 2023. in may 2026 it moved its official plugins from
GitHub release downloads to OCI artifacts on ghcr.io after repeated CDN failures.

## distribution

distribution now goes through OCI. the CNCF TAG Runtime working group's layout puts one `application/wasm`
layer under a config naming the target world, and consumers must reject artifacts with more than one
layer. the Bytecode Alliance archived its warg registry in july 2025 and pointed to wasm-pkg-tools, and
`wkg` dropped warg support on 2026-07-06. WIT packages map to OCI references with semver tags, and
`wkg.lock` records a digest per package. a component has no identity other than its bytes.

## where the systems differ

components or core modules is still contested. Zed, Spin, wasmCloud and one rustc draft use components.
Typst, Shopify, proxy-wasm, Extism, Helm, proto and the other rustc draft use core modules. the reasons
given for core modules are language coverage, runtime support outside rust, and the cost of a second ABI.

the projects that use wasm to make caching sound make no security claim: rustc lists it as a non-goal,
and Typst, Bazel and watt describe their goal as determinism and portability. Shopify and Zed run plugins
written by third parties and rely on the sandbox, and each adds something around it: Shopify its hard
limits, Zed its grants.

## result for this project

the decisions are recorded in [0071](../decisions/0071-host-bodies-bind-through-host-adapters.md) to
[0076](../decisions/0076-component-execution-is-deterministic-and-bounded.md).

for types, pith adopts the invariant through another mechanism. the `.pi` declarations stay the only
definition and the WIT world is generated from them. because identity at the boundary is structural, pith
checks every returned value against its declared type, which is also what Wasmtime's policy tells
embedders to do. recursive types still cross, since pith allows only direct self-reference and such a
value can be written as a node list and a root index. an `Int` crosses as a 64-bit case when it fits and as
bytes when it does not.

for determinism, pith adopts Typst's mechanism and closes its gap: no imports beyond pith's own protocol,
the deterministic profile's settings, fuel as the only execution bound, and a fresh store for each
computation. Shopify's split between a function and the effects it requests is one pith already has: a
body returns requests and action plans, and the engine and executor carry them out.

for authority, pith adopts Spin's deny-by-default. grants are written by the consuming project, per
dependency, from the first version. Zed added its grants eighteen months after launch, and Spin's
dependencies get either nothing or everything; neither is repeated.

rustc's wasm proc-macro proposal lists security guarantees as a non-goal. pith cannot take that position,
because it runs code from authors it does not know and caches what that code returns. the wasm sandbox is one
layer. the others are checking returned values, authorizing plans, confining actions, and running the
wasm runtime in a confined process. only tier 1 Wasmtime configurations run third-party code.

API versions follow Zed: `pith:core` is versioned on its own, each published version is frozen, and the
host converts results from older versions to current types. a component is content: the lock pins its
digest, OCI is a later transport, and precompiled artifacts are produced and read only on the machine
that compiled them.

pith uses components. WIT generation gives the `.pi` declarations a typed boundary, 0.3's async functions
fit the step protocol, and rust support is best there. the question stays open for later adapters, and
the argument in rustc draft #157709 about maintaining a second ABI should be revisited if the generator
turns out to be expensive to maintain.

three pieces of evidence are missing, and pith has to produce them itself: the cost of instantiating a
component per computation on pith's witness, the cost of crossing a process boundary on each step, and
how 0.3 async, which Wasmtime assigns no tier, behaves under fuzzing pith runs.

## sources

specifications and standards

- component model, design/mvp: https://github.com/WebAssembly/component-model/tree/main/design/mvp
  (Explainer.md, WIT.md, CanonicalABI.md, Concurrency.md, Binary.md), read 2026-09-29
- component model pull requests #489, #613, #705, #728 and issues #56, #398, #525:
  https://github.com/WebAssembly/component-model
- WASI releases and roadmap: https://wasi.dev/releases, https://wasi.dev/releases/wasi-p3,
  https://wasi.dev/roadmap, https://github.com/WebAssembly/WASI/releases
- WASI issues #942 and #943: https://github.com/WebAssembly/WASI/issues/942,
  https://github.com/WebAssembly/WASI/issues/943
- WASI 0.3 announcement, 2026-06-11: https://bytecodealliance.org/articles/WASI-0.3
- the road to component model 1.0, 2026-06-08: https://bytecodealliance.org/articles/the-road-to-component-model-1-0
- W3C proposals table: https://github.com/WebAssembly/proposals/blob/main/README.md
- WebAssembly 3.0 profiles: https://webassembly.github.io/spec/core/appendix/profiles.html
- nondeterminism: https://github.com/WebAssembly/design/blob/main/Nondeterminism.md

Wasmtime

- releases and release policy: https://github.com/bytecodealliance/wasmtime/releases,
  https://docs.wasmtime.dev/stability-release.html, https://bytecodealliance.org/articles/wasmtime-lts
- tiers of support: https://github.com/bytecodealliance/wasmtime/blob/main/docs/stability-tiers.md
- what is considered a security vulnerability:
  https://docs.wasmtime.dev/security-what-is-considered-a-security-vulnerability.html
- advisories: https://github.com/bytecodealliance/wasmtime/security/advisories and
  https://bytecodealliance.org/articles/wasmtime-security-advisories
- component API: https://docs.rs/wasmtime/latest/wasmtime/component/struct.InstancePre.html,
  https://docs.rs/wasmtime/latest/wasmtime/component/macro.bindgen.html,
  https://docs.wasmtime.dev/api/wasmtime/component/struct.Linker.html
- performance, 2022-09-06: https://bytecodealliance.org/articles/wasmtime-10-performance
- fast instantiation: https://docs.wasmtime.dev/examples-fast-instantiation.html
- deterministic execution: https://docs.wasmtime.dev/examples-deterministic-wasm-execution.html
- Crocus: https://cfallin.org/pubs/asplos2024_veri_isle.pdf
- wasmi: https://github.com/wasmi-labs/wasmi; WAMR: https://bytecodealliance.org/articles/wamr-announcement

systems

- Zed: https://zed.dev/blog/zed-decoded-extensions,
  https://github.com/zed-industries/zed/tree/main/crates/extension_api/wit,
  https://github.com/zed-industries/zed/pull/26224, https://github.com/zed-industries/zed/pull/39472,
  https://zed.dev/docs/extensions/capabilities
- Typst: https://typst.app/docs/reference/foundations/plugin/,
  https://github.com/typst/typst/releases/tag/v0.8.0, https://github.com/astrale-sharp/wasm-minimal-protocol
- Shopify: https://shopify.engineering/shopify-webassembly, https://shopify.dev/docs/api/functions/latest,
  https://shopify.dev/docs/apps/build/functions/network-access
- proxy-wasm: https://github.com/proxy-wasm/spec,
  https://www.solo.io/blog/the-state-of-webassembly-in-envoy-proxy-f2b3f
- Extism and Helm: https://github.com/extism/extism/discussions/334, https://extism.org/docs/concepts/manifest,
  https://helm.sh/community/hips/hip-0026/
- Spin: https://spinframework.dev/v3/writing-apps, https://github.com/spinframework/spin/releases/tag/v3.0.0
- wasmCloud: https://wasmcloud.com/blog/wasmcloud-v2-is-here/
- rust: https://github.com/dtolnay/watt, https://github.com/rust-lang/compiler-team/issues/1017,
  https://github.com/rust-lang/rust/pull/157590, https://github.com/rust-lang/rust/pull/157709
- Bazel: https://github.com/bazelbuild/bazel/releases/tag/8.3.0,
  https://github.com/bazelbuild/bazel/discussions/23487, https://github.com/bazelbuild/bazel/issues/26975
- proto: https://moonrepo.dev/blog/proto-v0.12, https://moonrepo.dev/blog/proto-v0.57

distribution

- CNCF wasm OCI artifact: https://tag-runtime.cncf.io/wgs/wasm/deliverables/wasm-oci-artifact/
- warg and wasm-pkg-tools: https://github.com/bytecodealliance/registry,
  https://github.com/bytecodealliance/wasm-pkg-tools/pull/193
