---
schema: design-doc/v1
id: decision-0041-the-written-lock
title: the written lock is a text projection of the lock value, and writing it is a caller effect
summary: a written lock is a text projection of a lock document value whose canonical bytes remain the only digest basis; the document carries the entries 0039 fixed with the feature coordinate 0040 added, the resolver revision, the declared version scheme, the preference list, and the candidate-universe digest; writing and reading the file are caller effects at the effect boundary, never rule behaviour; a read lock is both a record and an input, pinning as exact constraints with staleness a field diff that names the moved input; M-4 verifies bindings locally against read bytes and ships no witness
kind: decision
status: accepted
created: 2026-07-01
updated: 2026-07-27
tags:
  - packages
  - locks
  - provenance
  - artifacts
relations:
  informed_by:
    - research-dependency-resolution
    - research-artifacts-and-trust
  depends_on:
    - decision-0003-explicit-effects
    - decision-0014-reproducibility-properties
    - decision-0023-rule-and-cache-identity
    - decision-0024-persistent-engine-state
    - decision-0026-generic-typed-calculus
    - decision-0027-retention-and-gc
    - decision-0032-action-granularity
    - decision-0033-consumer-of-action-reuse
    - decision-0038-represented-rule-bodies
    - decision-0039-package-identity
    - decision-0040-declared-constraints-and-resolution
  supersedes: []
---

# the written lock is a text projection of the lock value, and writing it is a caller effect

> takes the two lines 0039 left open in this milestone, the lock's file format and its retention, and answers the witness question it deferred with what M-4's actual sources force. 0040's consequences already fixed part of what a written lock must carry; this record fixes what the artifact is, who writes it, and what reading it back does.

## context

0039 fixed the lock entry as a binding, coordinates to content with the origin as evidence, and drew the line this record crosses: the lock's file format, its retention, and the development environment are later records. 0040 fixed resolution as a host-rule computation in the graph and its consequences say what a lock written by one records: the preference list and the digest of the candidate universe it resolved against, alongside the entries. what remains is the artifact itself, a file in a repository that people read in review and merge under conflict.

the tension is real. pith has one canonical binary codec, and canonical bytes are what digests and reuse depend on; 0024 made that encoding a storage contract. every ecosystem that ships a lockfile ships text, and for reasons a design cannot dismiss: a lock diff is read by humans, and a conflict in a lock is resolved by a person. the precedents disagree about canonicality rather than about text. npm infers the lockfile's indentation and line endings from package.json, so the file's bytes are deliberately cosmetic. cargo exercises what its source calls strict control on the on-disk format, and still tells users to regenerate rather than hand-merge; its parser even heals a checksum table mangled by a bad merge.

they disagree more deeply about what a lock is for. go.sum is not a lockfile in the strict sense: selection lives in go.mod, and go.sum is a set of hash lines, union-mergeable, tolerant of extra entries, witnessed by sum.golang.org's transparency log with inclusion proofs. Cargo.lock and pnpm-lock.yaml freeze an environment, and pnpm redesigned its format three times with merge resistance as a stated goal, splitting per-project importers from shared resolution data so one project's change touches one section. poetry.lock carries a content-hash of pyproject.toml, which concentrates the merge pain in the one field a human cannot resolve by inspection; the issue asking for a merge-friendlier lock has been open since 2018. flake.lock is a JSON node graph pairing each input's original unlocked reference with its locked one and a narHash — a NAR serialization hash rather than a git tree hash, because it must cover file modes, symlink targets, and sorted entries; its node labels are arbitrary, so regeneration renumbers. Bazel's MODULE.bazel.lock records registry file checksums and extension digests, with a mode spanning update, refresh, error, and off. TUF's targets metadata is the same entry shape wearing threshold signatures. a lock can pin, witness, freeze, or cache; pith's answer comes from its own machinery rather than an average of theirs.

## proposed decision

### the lock is a document value, and the file is its projection

a written lock is a lock document: a value in the 0026 calculus that phloem carries the way it carries the entry. the document holds the entries and, beside them, the resolver revision, the declared version scheme, the preference list, and the digest of the candidate universe the resolution ran against. its identity is a digest over the value's canonical encoding, domain-separated the way every phloem digest is. comparing two locks is comparing digests, or the values underneath them.

the file is a projection of that value: rendered text, deterministic, line-oriented, one binding per line. the relationship runs one way on purpose. canonical bytes keep their existing job of feeding digests and the reusable index; the text takes the job canonical bytes cannot do, being read in a diff. a file whose bytes were rearranged is the same lock when the value underneath is the same, and a hand-edited file is normalized by rendering what it parses to.

### the round-trip guarantee

three commitments. render is a total deterministic function of the value, so the same lock renders to the same bytes in any process. parse inverts render on rendered output: parsing what was rendered yields the value back. and render canonicalizes parseable text: rendering what a hand-edited file parses to produces the canonical spelling, with entries back in sorted order and tokens back in their canonical quoting. what is deliberately not committed: the file's bytes are not canonical, and no digest is ever taken over them. npm's formatting-follows-the-manifest is the far pole and cargo's strict rendering the near one; pith sits near cargo in determinism while demoting the text from canonical, because canonical bytes already have a job and sharing it with the layout would couple identity to cosmetics.

the file's first line names the format version, pinned at 1 on the same terms the state store pins its own: nothing is released, and a format change breaks the file rather than migrating it. an unknown version is refused with the found version named, which is cargo's position and the cheapest honesty available. the body is a handful of header directives, the resolver revision, the version scheme, the universe digest, and one preference line per ordering in list order, then a blank line, then one binding line per entry sorted by the line's own bytes so adjacent packages stay adjacent in a diff. digests render as algorithm-prefixed hex, so the file names the digest algorithm it was written with, which is the agility lesson in-toto's digest sets carry. tokens that contain separators are quoted; comments run from a hash to the end of the line, and render never emits them.

merge behaviour follows from the shape; the format promises none of it. two branches adding bindings for different packages merge by union, a moved selection is a one-line diff, and a conflict in the header directives is the signal that the universe, the preferences, the scheme, or the resolver moved. the entries are a set over package identities, which is what makes the union safe: a byte-identical line the merge repeated collapses, and a second, different binding for a package already bound is refused when the file is read, naming both lines and both versions, because two selections of one package is the one conflict a union cannot represent and a person has to resolve. go.sum is the precedent for the line shape. no merge tooling ships with this record.

### what the document carries

the four protocol inputs and the rule half of the computation key.

the version scheme belongs in the lock because it is a request input and part of the computation key; resolve made it one on xylem's toolchain terms, and a lock that omitted it could claim reproduction under a universe whose meaning changed with the ordering. the preference list and the universe digest are 0040's clause. the resolver revision is the same argument one level up: 0023 separates a rule's identity from its revision because executable semantics move, and the selection is a function of the request under the revision as much as under the scheme. a lock whose entries moved while no recorded input moved has one explanation left, and the file should be able to name it. pnpm records the checksum of the .pnpmfile that may rewrite resolution, and Bazel records a digest over an extension's bzl files, for the same reason.

the entries carry the full coordinates. 0040 made features coordinates, so the entry 0039 spelled as a package version grows the feature field, and a lock that pinned versions but not features would pin half the selection.

two protocol inputs are excluded. the budget shapes whether the deterministic walk finishes, never what it selects, so a solved answer under a smaller budget is the same solved answer under a larger one, and a lock written from either is the same lock. the constraint set is excluded on a different ground: the lock records what resolution adds to the declarations, and the declarations are values the project already holds. poetry hashes its manifest into its lock because the lock cannot be compared to the manifest structurally; pith's staleness check compares values, and a hash over them would re-create poetry's most conflict-prone field to avoid a comparison the types already do.

### writing is the caller's effect

a resolution is a pure computation whose body sits in 0038's host-rule tier, and writing a file is not pure. the boundary is 0003's: the engine computes the answer value, and whoever drives the engine renders the document and writes the file. no rule sees a path, and the solver gains no file behaviour; the write and read functions are caller-side adapters in the phloem library, in the position the store adapters hold. the prototype holds this line with a test that resolves through the engine and observes that no file exists until the caller writes one.

a second ground appeared when the prototype was built, and it is recorded here rather than left in the code: the answer is not the whole document. 0040's protocol has an answer name the choice and the explanation, so the version scheme and the preference list are request-side values the answer does not repeat by design, and a lock is assembled from both halves. the caller is the only party holding the two, which is the same party the effect boundary names.

publication follows 0024's content discipline as the store implements it: a temporary file named for this writer and created exclusively, so two writers cannot share one; flushed; renamed into place; the destination directory flushed after the rename, so the rename itself survives a crash; and the temporary file removed on every failure path. only a solved answer yields a written lock. an unsatisfiable, underdetermined, or exhausted resolution writes nothing and the existing file stands, on the same ground 0024 uses when a failed reevaluation publishes no partial dependency set — the file records selections, and none of the three is one.

### reading a lock back: a record and an input

as a record, the file is the piece of resolution provenance that crosses processes as itself and survives outside both stores. as an input, the entries become pins: exact constraints over the coordinates each entry carries, attributed to the lock, fed to an ordinary resolution as its constraint set. a pinned re-resolution under the same universe reproduces the selection, and the reproduction is checkable rather than asserted because every input the selection depended on is either in the file or in the declarations the pins are checked against.

staleness is a field diff, not a re-hash. the document's header fields are compared against the request about to run, and a different scheme, universe digest, preference list, or resolver revision is reported by naming the field that moved. this is the same shape the ordinary invalidation explanation takes: the engine names the input that moved, 0033's machinery, and the lock's header exists so that comparison has something to read.

the binding check is per entry. a fresh resolution that selects the same coordinates must produce the same content identity, and different content under the same coordinates is 0039's drift, reported with both content identities and never absorbed, because it is either a new resolution to record or a domain breaking its immutability policy.

### the witness, answered for M-4

M-4 ships local verification only, and the deferral ends here: an entry's content identity is checked against bytes that were read; the origin records where the resolution happened in the terms the candidate's provenance carries; nothing else vouches for anything. the origins are honest about their limits. a path candidate records the path, which is exact. an archive candidate records the archive's own content identity as its locator, because an in-process universe names no registry and the registry client that would is out of scope. a git candidate refuses to yield an entry at all: a revision and a tree hash are a reference rather than content, and binding coordinates to content nobody read is exactly the shape 0014 separates from a measured fact.

what settles the witness question is the first remote source adapter, because that is when a threat model has an adversary to name. the candidates are already shaped by the systems that shipped them. a transparency log in go's arrangement witnesses the same line a lock entry is — coordinates to digest — with inclusion proofs and detection in place of prevention, and the author of a module cannot rewrite a version under it. TUF's threshold-signed metadata witnesses a repository's whole target set and buys freshness and rollback protection inside a closed key set. in-toto and SLSA attestations witness builds rather than bindings, a subject digest against a builder's signed claim, the right shape for 0039's binary-reuse admission test and the wrong one for a source pin. the entry's shape fits the log most naturally, and that is a prediction, not a choice; the adapter's threat model decides.

### retention

0027 governs the engine's two stores, and the lock file is in neither, which is the point of writing it. the file lives in the repository and its retention is version control's. the engine-side traces of a resolution, the attempts and edges under the computation key, follow 0027's ordinary axes and may be collected; the file survives that collection because it was never in a store, which makes it the durable half of the resolution's provenance. content collection does not touch it either: an entry's content identity is intrinsic in 0039's sense, valid wherever the bytes are found again, so a collected source is refetched rather than re-resolved. 0027 applies unchanged to what it governs and is irrelevant to the file by construction rather than by omission.

## alternatives considered

### the canonical bytes as the written form

one consumer of a lock file is another machine, and canonical bytes serve it with no parser at all.

rejected because the other consumer is a person reading a diff, and no binary format serves them. no surveyed system ships a binary lock, and the disagreement among the text ones is about canonicality, not readability. the digest position survives: canonical bytes remain the only digest basis, and they stop being the file.

### the text as itself canonical

digests taken over the file bytes, which makes the text and the value one artifact.

rejected because it makes cosmetic layout semantic. a reformat moves the digest, a hand merge breaks it, and canonicalization splits across two encoders that must then agree forever. it also couples the lock's identity to the text grammar's evolution, which is the faster-moving of the two.

### writing as an action in the graph

the write as an effectful computation the engine schedules, with the file as a declared output.

rejected on 0032's ground and on cost. an action is one invocation of one tool under a contract, and rendering text deterministically invokes nothing; the action's admission machinery, executor identity and platform, would stand guard over a rename. the boundary 0003 already draws puts the write with the caller at no cost, and a pure rule that touched a path would be the third class of state 0038 named, invisible to every key.

### the lock as a witness set only

go's split, with selection living entirely in the declarations and the file carrying hash lines that authenticate whatever is selected.

it buys union merges and loses the pin: nothing about a fresh resolution is constrained by the file, so a build cannot claim to run under the same lock. pith's entry was built as a binding, and a binding nothing checks is a log. rejected; the file pins, and the union behaviour is kept for the entries where it is compatible with pinning.

### a content-hash of the declarations inside the lock

poetry's content-hash, added to the header as a cheap staleness signal.

rejected because it duplicates a comparison the values already support and concentrates conflict in an opaque field, which is poetry's documented pain. staleness here names the moved input, which a hash cannot do.

### a structured whole-file document

JSON or TOML, as flake.lock and MODULE.bazel.lock ship.

rejected for the file on their merge evidence: both resolve conflicts by accepting one side and regenerating, and both bundle unrelated edits into one conflict. the line-oriented shape inherits go.sum's union behaviour for entries instead. the position is kept for the value: the document is a record value, and a structured projection of it is one renderer away if ever wanted.

## consequences

phloem gains the lock document value, its text codec, and the caller-side write and read; the entry gains the feature coordinate 0040 added. no kernel constructor lands, which makes this the second M-4 record to need none.

the engine is unchanged. resolution keeps producing a value, the reusable index keeps serving it under its key, and nothing in the engine learns that locks exist, on the terms 0039 set for the whole package library.

### measured

the prototype checks the record's claims directly: a resolution produces the document carrying the resolver revision, the scheme, the preference list, the universe digest, and the entries with features; the written form round-trips under the guarantee above, including canonicalizing a hand-reordered file; a lock read back binds the same coordinates to the same content and reproduces the selection when fed back as pins; two resolutions under the same candidate universe produce the same file bytes across separate engines and durable states; a changed universe, preference list, or scheme produces a different file whose diff names the input that moved; and resolving through the engine leaves no file until the caller writes one.

## unresolved

the witness for remote sources is narrowed, not open in the inherited form: the first source adapter's threat model picks among the log, the threshold, and the attestation, and the entry's shape predicts the log. until one exists, local verification is the whole claim.

no merge tooling ships. the union behaviour of entry lines is a property of the format; a git driver that exploits it is future tooling, and the format owes it nothing.

the file's placement and naming in a project, and whether one project holds one lock or several when it runs several resolutions, belong to the development-environment record this milestone still owes.

the post-release evolution policy for the text format is unset. pre-release it breaks freely on the state store's terms; when a lock becomes a stable contract, the migration question 0024 deferred for its own schema returns here for the file.
