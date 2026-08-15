---
schema: design-doc/v1
id: research-index-formats
title: index formats
summary: where five systems put a version's dependency requirements — cargo's index, Debian's Packages, Alpine's APKINDEX, PyPI's simple API, and the solvers that consume them — and what each placement costs
kind: research
status: researching
evidence: preliminary
created: 2026-07-17
updated: 2026-07-17
tags:
  - research
  - packages
  - registries
  - dependencies
relations:
  informed_by:
    - research-dependency-resolution
    - research-artifacts-and-trust
  depends_on:
    - research-method
  supersedes: []
---

# index formats

a dependency edge forces a question the single-package rounds could defer: where does a version's requirement live? the candidate a resolver reads has to say what it depends on, and the answer decides whether resolution needs fetches, what a registry can lie about, and what has to agree with what. the systems that shipped this read differently, and the disagreement is the substance — not a detail of encoding but a position on whether the index is a cache of what the archive says, an authority the archive must match, or the only place the fact lives.

this note reads the primary documents of five systems at that one question. it is the mechanism-level companion to `dependency-resolution.md`, which compared the solvers; here the question is what the solver is fed and by whom.

## cargo: the index duplicates the manifest, deliberately, with no written authority rule

cargo's index line is a JSON object per version, and it carries the dependencies: `"deps"` — "Array of direct dependencies of the package," each entry with `name`, `req` ("The SemVer requirement for this dependency"), `features`, `optional`, `default_features`, `target`, `kind`, `registry`, and `package` (the rename fields). beside it sits `"cksum"` — "A SHA256 checksum of the `.crate` file" — the registry's claim about the archive, computed by the registry itself: "The publish API does not specify the checksum, it must be computed by the registry before adding to the index."

the manifest rides inside the archive, so every dependency requirement exists twice, and the duplication is the design: RFC 2141 states the purpose — "Cargo needs to be able to get a registry index containing metadata for all crates and their dependencies available from an alternate registry in order to perform offline version resolution" — and RFC 2789's sparse protocol keeps it while shrinking the transport: "To learn about crates and resolve dependencies, Cargo (or any other client) would make requests to known URLs for each dependency it needs to learn about ... For each dependency the client would also have to request information about its dependencies, recursively, until all dependencies are fetched (and cached) locally."

what the documents do not say is which side wins when they disagree. there is no authority rule for a manifest conflict anywhere in the index docs or the RFCs. what exists instead is a set of adjacent statements: immutability by convention ("The JSON objects should not be modified after they are added except for the `yanked` field whose value may change at any time"), one field where the archive side is explicitly preferred ("Although `rust_version` is included here, [crates.io] will ignore this field and instead read it from the `Cargo.toml` contained in the `.crate` file"), a forward-compatibility rule for unknown schema versions, and — closest to a disagreement semantics — RFC 2789's "Dealing with inconsistent HTTP caches": "The index does not require all files to form one cohesive snapshot. The index is updated one file at a time, and only needs to preserve a partial order of updates," with recovery by cache-busting when a crate's dependency is not yet visible. the disagreement that matters to a lock — same coordinates, index and archive saying different things about requirements — is handled by never letting it matter: resolution reads only the index, and the manifest is read after the download, for the build.

## Debian: the index is generated from the packages, and must match exactly

Debian splits the fact across three places. the author declares `Depends` in the binary stanza of `debian/control` ("The debian/control file contains the most vital (and version-independent) information about the source package and about the binary packages it creates"); dpkg-gencontrol computes the installed `DEBIAN/control` ("reads information from an unpacked Debian source tree and generates a binary package control file ... during this process it will simplify the relation fields"); dpkg-scanpackages builds the repository index from the built packages ("sorts through a tree of Debian binary packages and creates a Packages file"). the index is a derived cache, and the derivation has a written contract, in the repository format page: "If the following fields exist in the control file of a .deb file they also must exist in the record about the package in the Packages file and the value must match exactly or a client might recognize a metadata mismatch and redownloads/reinstalls a package" — the fields being "Depends et al", `Installed-Size`, and `Multi-Arch`.

this is the only one of the five with a documented index-versus-package disagreement behaviour, and it is instructively weak: the sentence says what a client *might* do, not what it must, and the listed consequence is a redownload — an efficiency failure, not a security one. the source-package half lives one file over, in the `Sources` index ("They consist of multiple stanzas, where each stanza has the format defined in Policy 5.5 ... The 'Source' field is renamed to 'Package'"), so the binary dependency graph and the source dependency graph are two different indexes generated by two different tools.

## Alpine: the index is the only thing the solver reads

Alpine collapses the two sides by fiat of the tool: "The APKINDEX file contains a set of records extracted from the PKGINFO file of each package in the repository," and the package-info fields that get copied are named — "The package info metadata structure is the portion of package metadata which will be copied to the repository index when the package is being indexed. These fields will be available form the index even if the package is not installed." among them `depends` ("List of dependencies for the package. Installing this package will require APK to first satisfy the list of all its dependencies.") and `provides` ("List of package names (and optionally its version) this package provides in addition to its primary name and version"), which reach the index as the `D:` and `p:` lines — `P:` is the package name; the provides line is lower-case. the whole index is one signed artifact: "The index is served as APKINDEX.tar.gz ... The index is signed similarly to packages," and "Without a signature apk-tools will not trust the index file and will require the --allow-untrusted flag."

the solver's input is the index and the installed database, and the installed database is the index's shape again: "The installed file is a plaintext file of the same format as APKINDEX ... meant to be a faithful copy of the indexed data at the time the package was installed." the index is authoritative in practice because nothing else is consulted — the disagreement question never arises, since a package's interior metadata is not read at solve time. what that buys is one file to trust and sign; what it costs is that the archive-side metadata, once extracted, has no second life: there is no way to check a package against its index line beyond the signature over the index itself.

## PyPI: the index originally carried nothing, and the PEPs that fixed it chose a sidecar over the line

PEP 503's project page is one anchor per release file: "The href attribute MUST be a URL that links to the location of the file for download," with a hash fragment and at most `data-requires-python` and `data-gpg-sig` attributes. no dependency metadata at all — so a resolver that wanted to know a candidate's requirements downloaded the whole distribution: PEP 658 states the cost, "download multiple distributions of a project to choose from based on their metadata. This means they end up discarding much downloaded data, which is inefficient and results in a bad user experience," and the pre-658 default it replaced, "tools are expected to revert to their current behaviour of downloading the distribution to inspect the metadata."

the fix PEP 658 chose is not cargo's. instead of putting requirements on the project page, each file gains a sidecar: "the repository MUST serve the distribution's Core Metadata file alongside the distribution with a `.metadata` appended to the distribution's file name," pointed at by a `data-dist-info-metadata` attribute carrying its hash. the rejected-ideas section says why the requirements were not inlined: "it was proposed that repositories may directly include the information on the project page ... This approach was abandoned since a distribution may contain arbitrarily long lists of dependencies ... and it is unclear whether including the information for every distribution in a project would result in net savings since the information for most distributions generally ends up unneeded. By serving the metadata separately, performance can be better estimated since data usage will be more proportional to the number of distributions inspected." PEP 714 is a corrective footnote to 658, not a coverage extension — it renames the attribute (`data-core-metadata`, and `core-metadata` in PEP 691's JSON) because "PyPI did not support PEP 658 until just recently, which released with a bug where the `dist-info-metadata` key ... was incorrectly named" and pip had a matching crash, "a bug in pip [that] has existed since at least `v22.3`."

the sidecar is a third position: the index does not carry the requirement (the page stays lean), and the resolver still downloads per candidate — but kilobytes of metadata rather than megabytes of archive, still hash-checked by the index. the requirement's authority is the archive's own core metadata file; the index only vouches for its digest.

## the solvers: requirements arrive from whatever the index shape is

libsolv's input is repository metadata by construction — the README lists the supported repository formats ("rpmmd (primary, filelists, comps, deltainfo/presto, updateinfo); susetags ... apk"), and the tools that feed it convert indexes, not manifests: "The repo2solv tool converts repository metadata in the directory 'DIR' into a solv file." where duplicated metadata exists, libsolv resolves the duplication by policy, not by agreement: repositories carry a `priority` ("priority of this repo"), and pruning keeps the highest — "prune to repository with highest priority."

PubGrub's documented position is a provider interface and a laziness argument. dart's solver document: "when doing version solving, it's impractical to eagerly list all dependencies of every package. What's more, the set of packages that may be relevant can't be known in advance," so "Pubgrub adds only the formulas that are relevant to individual package versions, and then only when those versions are candidates for selection." the pubgrub-rs guide makes the sourcing explicit and open: "a dependency provider may need to retrieve package information from caches, from the disk or from network requests," with `get_dependencies` returning unknown when not yet fetched, and caches justified because "Since dependencies are generally immutable, caching them is a valid strategy to avoid slow operations that may be needed such as fetching remote data." dart's own hosted registry puts the manifest inside the version listing — the per-package API returns, per version, `"archive_url"`, `"archive_sha256"`, and `"pubspec"` — so in the system PubGrub was born in, the requirements are served beside the version list, cargo-style, not inside the archive.

CUDF, the format the MISC solvers competed over, simply forbids duplication: "There may be at most one package description stanza for any given pair of package name and version," and duplicated property names are parse errors — a universe document, not a registry-plus-archive pair, which is why it could stay silent on disagreement.

## what the disagreement is

read together, the five are not variations of one answer. cargo duplicates requirements in the index so resolution needs no fetches, and writes no authority rule for the disagreement its duplication makes possible. Debian derives the index from the packages and states an exact-match expectation whose enforcement is left to clients that "might recognize" a mismatch. Alpine defines the index as a copy of package metadata, signs the copy, and has the solver read only the copy — the disagreement is impossible because the second source is unreachable. PyPI keeps requirements out of the page and out of the archive's hash-anchored position on it, serving them as per-file sidecars the index vouches for but does not contain. and the solvers are indifferent to the choice — they consume whatever arrives, with libsolv adding priority policy for the duplicated case and PubGrub hiding the sourcing behind a provider interface.

the axis underneath: whether the index is a cache (Debian, generated, exact-match expected), an authority (Alpine, signed, exclusively consulted), or a pointer (PyPI, sidecars with digests) — with cargo the pragmatic fourth that duplicates for speed and leaves the conflict undefined. the costs also separate: fetch-free resolution (cargo, alpine) against download-per-candidate (PyPI before 658), one signed file against a per-field contract, and a defined failure mode against none.

for a pith index line the relevant pressures are: requirements belong where the resolver reads them, so a resolution is one read and never a fetch; the index's claim about an archive is already a digest the fetch verifies (0044's shape), so a requirement in the index participates in the same trust position — the log witnesses bindings, the resolution trusts the index for the graph; and the disagreement surface should be named before it exists — Debian's exact-match sentence is the honest precedent, cargo's silence the cautionary one.

## sources

- [cargo registry index format](https://doc.rust-lang.org/cargo/reference/registry-index.html)
- [RFC 2141: alternative registries](https://rust-lang.github.io/rfcs/2141-alternative-registries.html)
- [RFC 2789: sparse registry](https://rust-lang.github.io/rfcs/2789-sparse-index.html)
- [crates.io-index README](https://github.com/rust-lang/crates.io-index)
- [Debian repository format](https://wiki.debian.org/DebianRepository/Format)
- [Debian policy 5: control files and fields](https://www.debian.org/doc/debian-policy/ch-controlfields.html)
- [Debian policy 7: relationships between packages](https://www.debian.org/doc/debian-policy/ch-relationships.html)
- [deb(5)](https://manpages.org/deb/5), [deb-control(5)](https://manpages.org/deb-control/5), [dpkg-gencontrol(1)](https://manpages.org/dpkg-gencontrol/1), [dpkg-scanpackages(1)](https://manpages.org/dpkg-scanpackages/1)
- [Alpine Apk_spec (the live redirect target of Apkindex_format)](https://wiki.alpinelinux.org/wiki/Apk_spec)
- [archived Apkindex_format](https://web.archive.org/web/20230601000000/https://wiki.alpinelinux.org/wiki/Apkindex_format)
- [apk-package(5)](https://github.com/alpinelinux/apk-tools/blob/master/doc/apk-package.5.scd), [apk-index(8)](https://github.com/alpinelinux/apk-tools/blob/master/doc/apk-index.8.scd), [apk-world(5)](https://github.com/alpinelinux/apk-tools/blob/master/doc/apk-world.5.scd)
- [PEP 503: simple repository API](https://peps.python.org/pep-0503/)
- [PEP 658: serve distribution metadata](https://peps.python.org/pep-0658/)
- [PEP 691: JSON simple API](https://peps.python.org/pep-0691/)
- [PEP 714: rename dist-info-metadata](https://peps.python.org/pep-0714/)
- [libsolv README and docs](https://github.com/openSUSE/libsolv)
- [dart pub solver document](https://github.com/dart-lang/pub/blob/master/doc/solver.md)
- [pubgrub-rs guide](https://pubgrub-rs-guide.netlify.app/)
- [dart hosted repository spec v2](https://github.com/dart-lang/pub/blob/master/doc/repository-spec-v2.md)
- [CUDF 2.0 specification](https://www.mancoosi.org/reports/tr3.pdf)
