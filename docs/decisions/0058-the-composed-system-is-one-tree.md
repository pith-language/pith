---
schema: design-doc/v1
id: decision-0058-the-composed-system-is-one-tree
title: the composed system is one tree published by one action, the merge ships agree-concat-and-overlay, and M-5a's kernel findings are a capture fix and three measured syscalls
summary: the system library M-5a opens is stele, a peer crate whose merges, renders, and one assembly action compose files, users, a service, and boot configuration into one canonical tree; the value layer needed no constructor for it, the milestone drove no encoding version, and what it did drive is argued here, with capture emitting Symlink entries and symlinkat, copy_file_range, and chdir joining the allowlist, each measured
kind: decision
status: proposed
created: 2026-08-18
updated: 2026-08-18
tags:
  - composition
  - libraries
  - systems
  - executor
relations:
  informed_by:
    - research-system-composition
    - decision-0028-sandboxed-local-executor
    - decision-0030-toolchain-closure-as-declared-input
    - decision-0032-action-granularity
  depends_on:
    - decision-0052-the-merge-operator
    - decision-0009-peer-first-party-domains
    - decision-0021-arena-graph-engine
    - decision-0045-a-locked-source-becomes-a-built-artifact
  supersedes: []
---

# the composed system is one tree published by one action, the merge ships agree-concat-and-overlay, and M-5a's kernel findings are a capture fix and three measured syscalls

> takes the milestone's standing framing — 0021's design criterion is the thing M-5a exists to test, and a kernel change it demands is the round's most interesting finding, argued in a record rather than assumed into the diff — plus the two items 0052 left to the prototype round: the operator's exact signature and its policy constructor set. 0052 stands; its operator ships here. the milestone text's other prediction, that output-tree capture's symlink dereference stops being theoretical in a domain whose /etc is mostly links, is the finding it turned out to be.

## context

M-5a composes files, users, a service, and boot configuration into one immutable Linux artifact, and its statement says the artifact runs "on the kernel as it stands" — the first domain whose shapes the calculus was not extended for, which makes it the first real test of "a new domain implementable without a core patch." the convergence tracking M-4 began asks every milestone to name which constructors and encoding versions its domain drove. M-4 drove `Record` and `Sum` on the day its own statement claimed none; 0055 moved `Int` on 0018's ground rather than a milestone's. this round is the first entry with nothing to move, and the finding is what moved instead: two changes in the first-party executor, each predicted, each measured.

three things in the tree were load-bearing here rather than latent. the capture path dereferenced symlinks, so no action could ever produce a `Symlink` entry, and `symlink` and `symlinkat` sat outside the seccomp allowlist, so no confined child could create one either. and 0052's merge operator existed as a decision with no code, its signature and policy set deferred to the library this milestone opens.

## proposed decision

### the artifact is one canonical tree, published by one action

the [system-composition note](../research/system-composition.md) found four of five systems collapsing to a tree and the fifth paying for a sequence with whiteouts, so the artifact stele composes is one tree in the store's own model — identity over sorted entries, executability and symlink targets hashed in, the OSTree construction. layers, where a domain wants them, are a projection from that tree, and assertions over a live machine belong to M-5b.

the library is `crates/stele`, named for the stem's central cylinder, the structure that holds every tissue in one axis. it registers seven pure rules and one action rule through `register_rule` and `register_action_rule`: three merges (etc, users, unit), three renders (unit file, passwd, boot entry), the assembly action, and the entry that requests them. everything the artifact's identity rests on is decided above the action, where a disagreement is a diagnostic and no process starts; publishing is the action's because content enters the store only through capture, which is 0045's ground measured a second time.

the assembly action is one tool invocation under 0032: a shell whose supervisor role 0028 already measured, running a script the planner derives from the canonical file set — one `mkdir -p` for the directories, one `cat` per staged file, one `printf` per rendered text, one `chmod` per declared mode, one `ln -s` per symlink. the script is a derived fact of the contract, in its arguments and its digest and `plan_action`'s answer. staged bytes live under a `pool/` tree disjoint from the `system/` output tree because the contract's own validation refuses input and output paths that overlap — an action produces its output, it does not capture what the executor staged into it. the three rendered texts cross as environment variables, NUL-checked at plan, the way a caller effect's bytes reach a program without a planner that reads the filesystem.

### the merge operator ships: agree, concat, and the keyed overlay

0052's deferred signature lands as three functions in `stele::merge`. `merge_records` composes field by field under a declared policy whose constructor set is two: `agree`, every carrier must hold the same value, and `concat`, every carrier's list accumulates into the sorted duplicate-free order the constructors already canonicalize to. a field the policy does not name must agree — fail-closed is the default, so accumulation is declared at the merge site, which is 0026's own sentence. `merge_keyed` is the same algebra at key granularity, file sets by path and user tables by account name. `replace_field` is C-3's operation: it names the field, the owner whose declaration it replaces, and the value, fails when that owner no longer declares the field, and installs the value on every carrier so the merge below agrees by construction — the winner is visible at the merge site rather than picked from an order.

two spelling consequences fell out of closed records. every fragment of a unit spells every field, so "who owns a field" cannot mean "its sole declarer"; it means who declares it at all, and the replacement's ownership check is against that set — the honest form C-3 can take when the type system refuses partial fragments. and one canonical list order had to be shared: the constructors and `concat` both canonicalize through `types::canonical_list`, sorted by canonical encoding, because the first draft sorted texts lexicographically in one place and by encoding in the other, and the merge test caught the two orders disagreeing on `After=`.

the policy and the replacements are request inputs, so two merges under different policies or replacements are different computations on 0023's terms, measured below.

### the convergence finding: nothing moved in the calculus, and the executor widened

stele declares twelve types over constructors that all predate it — nominals over lists and records and texts, one payload-carrying sum for file bodies, one bare sum for behaviors — and drives no constructor, no engine change, and no encoding version. the artifact's value is a nominal over a content identity: there is no `Value::Tree`, and the argument against adding one is 0047's gate. tree-ness lives where the kernel already keeps it, in the store's tree model and in the contract's declared tree output, and a content id is everything M-5b's activation half will need to name a composed system. phloem's `merge_provided` stays a domain-local function; 0052's escalation trigger is unchanged and now has a second consumer measured beside it.

what the milestone demanded is in `pith-executor-local`, and the milestone text predicted both halves. capture reads a symlink's target instead of following it, so a link survives as declared content — a dangling link included, since `read_link` does not care and neither does the manifest. and the allowlist admits three syscalls, each named by the concrete failure that produced it, the discipline 0028 fixes. coreutils `ln -s` creates through `symlinkat` (the `at` form, measured; the plain form never appeared in a trace), coreutils `cat` splices with `copy_file_range` before falling back to `read`/`write`, and coreutils `mkdir -p` with several operands walks relative components with plain `chdir` — the same movement `fchdir` was already admitted for, confined by the ruleset the same way, and spelling-dependent denial was the only reason it was absent. the filter now admits 80 syscalls, 78 unconditional and two argument-filtered, against 77 when the milestone opened.

a third executor fact was measured rather than changed: 0030's finding that a toolchain is a closure, not a binary, reappears verbatim on the system tools. the assembly's confined child could not start — `EACCES` at `execve` — until the tools record carried the closure the loader opens, the interpreter above all, and the library now ships `discover::tools_closure` on the same terms the executor's own fixtures discovered closures: exact under `/nix/store` where nix records the answer, loader-trace best-effort elsewhere. the Tools value carries the closure as a declared input, so it enters the contract and the computation key the way 0030's closures do.

## alternatives considered

### a `Value::Tree` constructor for the artifact

the artifact is a tree, so the value layer could say so: a constructor carrying a tree identity or an entry structure, and rules passing trees as first-class values.

rejected on 0047's gate, which this milestone exists to test. nothing reads a tree value structurally — the assembly stages blobs and creates links, the renders read records, M-5b will materialize from the store — so a `Tree` constructor would be the `Type::Nominal` history again: a shape with no reader. OSTree, the purest tree system in the note, keeps tree-ness in its object model and passes content ids around; pith's kernel already has that object model. the nominal over a content id says everything the graph needs.

### a layered artifact, the OCI shape

compose the system as an ordered layer sequence, content-addressed as a sequence, with the tree a derived view.

rejected on the note's own finding. OCI's append sequence pays for layering with whiteouts — "a special filename that signifies a path should be deleted" — because a sequence has no operation that reaches back, and the four tree systems among the five converge on one canonical tree with the overlay confined to construction, BuildStream's position. a layered export of a stele tree is a projection a later round can derive; the whiteout mechanism is not a semantic to inherit.

### caller-side assembly, the unpack pattern

compose the tree where 0045 unpacks an archive: a pure rule computes the file-set value, and the caller materializes it to a directory as a caller effect.

rejected because the artifact must be graph-identified and reusable. 0045's measurement — "a realization is the attempt the engine already holds," `Reused` on the second build, `Hydrated` in a fresh engine — is exactly the property M-5b needs from a composed system, and a caller-side directory is neither content-addressed nor invalidatable. publishing through an action gets the artifact a content identity, the assembly its own reusable computation, and a fragment edit its measured delta.

### one action per symlink, or per file

`ln` is one tool invocation; a /etc of many links could be many actions, each its own cache entry with the whole tree staged under it.

rejected on 0032's boundary. the milestone statement already answers it — "the tools that turn a tree into an image are ordinary actions under 0032's granularity" — and the compiler precedent is measured: `cc` execs `cc1` and `as` under one action because the driver is a supervisor. the shell assembling one tree is the same shape, one invocation of one program with sub-tools, and N actions each staging the whole artifact would spend quadratic content to keep the same identity.

### keep dereferencing capture, and stage links as files

the status quo: capture follows symlinks, an /etc of links becomes an /etc of copies, and the artifact still works.

rejected because the artifact stops being the thing it composes. OSTree hashes the symlink target into the entry's identity — "symbolic link target (for symlinks)" in the content-object header — and the HotOS paper's machine is "/etc consists almost entirely of symlinks." a copy is a different artifact: it drifts from its target, materializes the wrong thing, and tells M-5b's activation half lies about what it installs. the note predicted this is where the asymmetry stops being theoretical; it did.

## consequences

the workspace has a fourth domain, layered on the kernel alone like xylem and the example fixture, and its two workspace lines — the member entry and the lockfile entry cargo derives — are again the whole carrying cost. 0056's tree-reading test now covers three other crates and passed without an edit, catching only this crate's prose naming the fixture domain, which is the failure it exists to catch.

C-2 is discharged by the posture and its tests: conflicts refuse naming field or key and both owners, and the merges are functions of the set of contributions. C-3 is discharged by replacement, in-graph as a request input. C-7 is discharged by the policy being a declared value and the order-insensitivity property. C-4's provenance half stays open where 0052 left it — what a merge records per field about where each value came from — and the merged values and refusal diagnostics carry what the prototype uses.

the executor's confinement story is wider and still measured: every allowlist entry names a need, the three new ones name theirs, and the capture change makes the executor's output model agree with the store's, which had carried `Symlink` entries end-to-end since M-2. no encoding version moved, on 0048's rule: the Tools record's closure field is a new declared input shape, and pre-release incompatibility is a rebuild.

stele stamps `StableCode(9006)`, the fourth domain to pick from the 9000 range by reading the others, and the allocation question 0053 and 0056 left open is now cheaper to answer and more expensive to defer. the read-back gap 0056 named is hit again: this crate's tests open second handles on the content store for `get_blob` and `get_tree`, and a library consumer would too.

### measured

the crate is 2915 lines of non-test source across ten modules — `types.rs` 1182, `rules/assemble.rs` 506, `merge.rs` 295, `rules/mod.rs` 298, `rules/render.rs` 170, `rules/unit.rs` 151, `discover.rs` 127, `lib.rs` 78, `rules/etc.rs` and `rules/users.rs` 54 each — plus 1654 lines of tests and fixtures. its `[dependencies]` are `pith-diag`, `pith-ids`, `pith-core`, `pith-engine`; it names no other domain. `git diff` over `pith-core`, `pith-engine`, `pith-ids`, `pith-diag`, `pith-arena`, `pith-store`, and `pith-state-sqlite` is empty for the round: the convergence claim is a diff, not an assertion.

the three tests 0052's measured section named as owed all exist and fail honestly. `a_disagreement_names_the_field_both_values_and_both_owners` drives two unit contributions apart on `exec` and asserts the refusal spells the field, both values, and both `base` and `machine`. `a_replaced_field_whose_ownership_changed_fails` replaces a field as an owner who no longer declares it and gets the current declarers named back; `a_replacement_resolves_a_contested_field_and_names_the_winner` drives the agreeing merge to the replaced value. `permutations_of_contributions_merge_to_one_result` permutes three contributions under a concat policy and gets one canonical result, and the overlay refusal and the user-disagreement refusal are tested at their own granularity. ten tests in `tests/merge.rs`.

the composition suite is nine host-agnostic tests over a portable fixture executor that stages the blobs, runs the contract's own script through the host shell, and walks the tree back — claiming `Unverified`, since it installs no confinement. a cold compose is `Computed` and the artifact carries what the fixture declares: hosts bytes, passwd and unit and boot texts byte-exact against their expected renderings, both symlinks with verbatim targets, and the declared executable bit on a profile script. the second request is `Reused` with the executor's execution count still one, and a fresh engine over the same root is `Hydrated` and plans nothing. the same contributions listed in both orders are one request — the second `Reused`. a unit conflict fails with this domain's code and zero action computations. the policy decides: unlisted fields must agree and refuse, the concat policy merges and renders both `After=` targets. a replacement reaches the artifact's unit text and a stale one names who declares the field now, with exactly one action planned across both. `plan_action` returns the derived script — `set -eu`, the quoted `mkdir -p`, one `cat` per staged file under `pool/`, the three `printf` lines, `chmod +x`, the two `ln -s` lines — with the closure canonically sorted and the texts in the environment. and the granularity claim is a count: editing one unit fragment recomputes exactly three pure computations — its merge, its render, and the entry — plus one action, while the etc and user merges and their renders are served.

the confined suite is three linux tests over the first-party executor and real tools resolved from the host. `a_confined_assembly_produces_symlinks_that_survive_capture` composes under landlock and seccomp, walks the artifact from the store, asserts both links and the declared mode, then materializes the tree and reads a link with `read_link`, reads through a resolving link, and gets `ENOTDIR` through a dangling one. the second compose is `Reused` and a fresh engine is `Hydrated`, both with the same artifact identity. and two cold engines over two separate roots compose to the same identity — the artifact is a function of its declared inputs. the executor crate's own suite grew `tree_output_symlinks_are_captured_as_entries_not_dereferenced`, which drives a real `/bin/sh` child through `cat`, `ln -s`, and a dangling link, so each allowlist entry added this round has a failing witness without it: the confined suite measured `SIGSYS` (exit 159) on `chdir` and `EACCES` on `execve` before their entries and closures existed.

## unresolved

the 9000-range allocation, now extended by a fourth domain. the trigger 0056 named — cheap now, a compatibility break at five — is one round away from being the cheapest record in the file.

the provenance shape for per-field ownership, 0052's unresolved item, unchanged: the merged values and the refusals carry ownership information, and nothing records it structurally yet.

tool discovery as a library surface. `discover::tools_closure` is exact under `/nix/store` and loader-trace best-effort elsewhere, the same limit the executor's fixtures record; whether the system library owns discovery the way xylem owns `Toolchain::discover`, or a caller does, wants the first non-nix host that runs the suite.

the boot entry is a rendered projection in the Boot Loader Specification's shape, not a bootloader integration: nothing signs it, no loader reads it, and M-5b decides what an installed artifact does with one. the service question is open in the same way — a unit here is a record rendered to systemd's main-file format, and the open question "what is the semantic definition of a service without importing systemd assumptions" stays where it was.

replacement scope: the in-graph spelling carries text values, so it replaces scalar fields; a list field is replaced by redeclaring its contribution. whether a list replacement earns its own value shape wants a consumer that needs one.
