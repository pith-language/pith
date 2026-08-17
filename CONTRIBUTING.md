# Contributing to Pith

Pith is a working prototype with one maintainer. The design notebook is the
real specification, and this guide says what a change here has to engage with.

## Where to start

Open an issue before a pull request. If a change disagrees with a decision
record, argue with the record itself, in the open, before writing code.

Fixes to what runs today are welcome, and so are tests that demonstrate a
claim, gaps the decision records already admit, corrections to the notebook,
and close readings of the systems this project learns from. Features in
territory the [milestones](docs/planning/milestones.md) mark as proposed are a
different matter: a proposal there starts as a record.

## The notebook comes with the code

The [decisions](docs/decisions) are numbered records, each closed by a
measurement. Design documents follow the `design-doc/v1` front matter (id,
title, summary, kind, status, created, updated, tags, relations), which
`just ci` validates. A contribution that changes a decision updates the
record; one that introduces a decision writes the next number.

## Working in the repository

The development environment is pinned with Nix.

```sh
nix develop
just test
```

`just` lists the commands. `just check` runs formatting and static analysis;
`just ci` runs the full local suite. Snapshot tests use insta: a pending
snapshot lands as `.snap.new`, and an accepted one is committed as `.snap`.
Read the diff before accepting. The Linux executor and system fixtures
compile to zero tests elsewhere, so a green run on another platform has not
exercised those paths.

## What a change carries

Commits follow `type(scope): summary`, as in `feat(xylem):` or
`docs(planning):`, with a lowercase summary stating what the change does.

The person submitting writes the pull request description: what changes, why,
and what holds it to account. A behavior change comes with the test that
demonstrates it.

`just ci` has to pass. Warnings are denied, formatting is checked, and the
repository runs its own checks: unordered hash collections are forbidden in
crate source (decision 0021), so use `BTreeMap` and `BTreeSet`.

## Authorship and AI tools

This project is written with generative-AI tools:

- coding agents draft code, tests, and text under direction; every change is
  read, judged, and accepted by a person;
- the copyright and license claims in [LICENSE](LICENSE) and [NOTICE](NOTICE)
  are made by a human on that basis.

Contributions are held to the same terms, as three rules.

1. Disclose substantial assistance. If a generative-AI tool produced a
   substantial part of what you submit, say so in the pull request or as a
   commit trailer:

   ```
   Assisted-by: <tool and model>
   ```

   The label is there so review can be honest about where the code came from.

2. Stand behind every line. What you submit must be yours to license under
   Apache 2.0, you must have read all of it before asking anyone to review
   it, and you must be able to answer for why each change is there. A tool
   regenerating copyrighted material does not remove the copyright.

3. Do not send work you cannot vouch for. Reviewer time is the scarcest
   resource a small project has, and a contribution should be worth more to
   the project than the time it takes to review. Autonomous agents opening
   pull requests without a person driving them are not welcome.

## License

Pith is licensed under [Apache 2.0](LICENSE), and contributions land under
the same license. Attribution notices for third-party work are collected in
[NOTICE](NOTICE); a contribution that bundles third-party work adds to that
file.
