---
schema: design-doc/v1
id: research-system-composition
title: composing a system from declared parts
summary: how NixOS, Guix, OSTree, BuildStream, and OCI image layers each compose a filesystem image from declared configuration, and what each answers to the question of whether a composed system is a tree, a layered sequence, or a set of assertions over a target
kind: research
status: researching
evidence: reviewed
created: 2026-08-05
updated: 2026-08-05
tags:
  - research
  - composition
  - systems
relations:
  informed_by:
    - research-configuration
    - research-nix
    - research-deployment-and-state
  depends_on:
    - research-method
  supersedes: []
---

# composing a system from declared parts

M-5a composes files, users, a service, and boot configuration into one immutable Linux artifact. before building that, this note reads five systems that already do it, from their primary documents: NixOS modules and `system.build.toplevel`, Guix's operating-system and service fold, OSTree's commit model, BuildStream, and OCI image layers.

the question the plan names is what shape the composed thing has: one tree, an ordered sequence of layers applied over each other, or a set of assertions applied to a target machine. reading the five settles a second question the first one hides: the shape of the artifact and the conflict rule of composition are independent choices. systems that agree on the shape disagree completely on what happens when two declared parts claim the same slot, and that disagreement is where M-5a's merge operator has to take a position.

## NixOS: a fixed-point merge, then a tree of symlinks, then an activation script

the pressure is recorded in the JFP paper's own footnote on the pre-module design: "Prior to the development of NixOS's module system, the Nix expression defining the firewall derivation had to use the option services.sshd.enable to decide whether to include port 22. This hurt extensibility and violated the principle of separation of concerns." the module system exists so that cross-cutting concerns can be declared where they belong and combined by the system.

the mechanism is that merging belongs to options, and every option carries a type that carries its merge. the manual: "When multiple modules define an option, NixOS will try to merge the definitions." `types.listOf`: "Multiple definitions are merged with list concatenation." `types.attrsOf`: "Multiple definitions result in the joined attribute set." `types.lines`: "A string. Multiple definitions are concatenated with a new line `\"\\n\"`." and for scalars, `types.bool`: "All definitions must have the same value, after priorities. An error is thrown in case of a conflict."

the phrase "after priorities" is the load-bearing part. each definition can carry a number, and the merge only sees the definitions with the best one. the manual, in "Setting priorities":

> A module can override the definitions of an option in other modules by setting an *override priority*. ... All option definitions that do not have the lowest priority value are discarded. By default, option definitions have priority 100 and option defaults have priority 1500.

`lib.mkForce` is `mkOverride 50` and `mkDefault` is `mkOverride 1000`; the source in `lib/modules.nix` also carries `mkVMOverride` at 10 and `mkImageMediaOverride` at 60. the ladder is open-ended, and the error thrown when two equal-priority definitions of a scalar conflict prints a suggestion to reach for `mkForce` or `mkDefault`.

the merged configuration is fed back into every module: "The full configuration resulting from the merge is passed as an input back into each module through the config function argument," a circular definition made safe by laziness, with `mkIf` pushing conditionals down to individual definitions so the fixed point is not forced early.

what the merge produces is a tree. the JFP paper again: "the value of the option system.build.toplevel is a derivation that simply creates symlinks to its inputs, e.g. $out/kernel links to the kernel image, $out/activate links to the activation script, and so on." the builder in `top-level.nix` is literally `ln -s` lines — `ln -s ${config.system.build.etc}/etc $out/etc`, `ln -s ${config.system.path} $out/sw` — and the /etc it links to is itself built as a store tree of symlinks from the `environment.etc` option. the HotOS paper states the resulting machine plainly: "Aside from a single exception, there is no /bin, /usr, /lib, etc. in this system, and /etc consists almost entirely of symlinks to generated configuration files in /nix/store."

the tree does not touch the machine by itself. `nixos-rebuild switch` installs the toplevel as a new numbered generation of the system profile (`/nix/var/nix/profiles/system-N-link`, each a garbage-collector root), then runs `switch-to-configuration`, which updates the bootloader, runs the activation script (whose last act is `ln -sfn "$(readlink -f "$systemConfig")" /run/current-system`), and reconciles the running systemd against the new units — the manual's chapter on a system switch lists eleven ordered actions, from "Stop units (`systemctl stop`)" through "Restart units (`systemctl restart`)", and notes the process "takes two data sources into account: `/etc/fstab` and the current systemd status."

so NixOS is a tree at the identity layer, with priorities at the merge layer, and a convergent imperative step at the machine. the paper's own account of the last part is honest about its status: "This is a slight blemish on NixOS's purely functional model: activation, like deployment in Nix, is not atomic, and no rollback of activation is provided." what the model does claim is congruence: "Disregarding mutable state, NixOS has a congruent model: after a nixos-rebuild, the system is in a state determined by the NixOS system configuration specification, and independent from the previous state of the system."

## Guix: a typed fold over a service graph

Guix keeps NixOS's artifact shape and replaces the merge algebra. an entire machine is one Scheme value: "all aspects of the global system configuration—such as the available system services, timezone and locale settings, user accounts—are declared in a single place," the `operating-system` record passed to `guix system`.

composition happens through extensions rather than through a global option namespace. the manual, "Service Composition":

> Guix system services are connected by extensions. ... All in all, services and their 'extends' relations form a directed acyclic graph (DAG). ... At the bottom, we see the system service, which produces the directory containing everything to run and boot the system, as returned by the guix system build command.

each service type declares extensions naming their target type and a procedure computing its contribution; the target type declares `compose`, which reduces the list of contributions, and `extend`, which folds the reduction into the target's own value. the shepherd root service in `gnu/services.shepherd.scm` is the canonical fold point, with the comment "Extending the root shepherd service (aka. PID 1) happens by concatenating the list of services provided by the extensions." the profile and /etc services compose by `concatenate` and extend by `append`.

conflicts are errors, stated statically. a duplicate shepherd name raises "service '~a' provided more than once"; a duplicate /etc entry raises "duplicate '~a' entry for /etc"; and two instances of one extensible type are refused because "the service-extension specifications would be ambiguous." i found no priority or override mechanism anywhere in `gnu/services.scm` or the manual's service chapters — the verified absence is scoped to those. when a user must change what `%base-services` contributes, the manual's answer is surgery on the list before the fold: `modify-services` with `delete` clauses and rewriting clauses. resolution is redeclaration, and it happens outside the merge.

the artifact is the same shape as NixOS's: `guix system build` returns "the derivation of the operating system, which includes all the files and programs needed to boot and run the system," one store directory. activation differs in one detail worth keeping: `activate-etc` "Install[s] ETC, a directory in the store, as the source of static files for /etc" — per-entry symlinks installed into a live /etc, where NixOS increasingly remounts /etc as an atomically-replaced overlay of the store tree. and reconfigure is generation-shaped like `nixos-rebuild switch`: "Build the operating system described in file, activate it, and switch to it," a new numbered generation, `/run/current-system` repointed, services not running started and running ones "arrange[d] ... for it to be upgraded the next time it is stopped."

## OSTree: the tree is the identity

OSTree is the purest tree answer. "OSTree is deeply inspired by git; the core layer is a userspace content-addressed versioning filesystem," with commit objects referencing "a dirtree/dirmeta pair of checksums which describe the root directory of the filesystem." content objects hash their metadata into identity: "its content objects include uid, gid, and extended attributes (but still no timestamps)," and "The header contains uid, gid, mode, and symbolic link target (for symlinks), as well as extended attributes. ... These parts together form the SHA256 hash for content objects." a symlink is a first-class entry whose target participates in the digest.

deployments are checkouts, and checkouts are cheap because they share storage with the repository: "the deployment directories have no files at all in them — they are entirely hardlink farms." atomicity comes from never mutating a live tree: "To swap the contents atomically, if the current version is 0, we create `/ostree/boot.1`, populate it with the new contents, then atomically swap the symbolic link `/boot/ostree/boot.0`." with staged deployments the whole transaction moves to boot time: "OS updates are fully transactional: staged deployments, config rolling forward or back, and the bootloader configuration are all updated atomically at boot time."

the tree discipline has exactly two exceptions, and they are the same two everywhere in this note: "OSTree supports exactly two persistent writable directories that are preserved across upgrades: `/etc` and `/var`." /etc is reconciled by a 3-way merge at deployment, which is why the staged-deployment feature exists — otherwise "the 3-way `/etc` merge is delayed until the system is rebooted or shut down."

static deltas are the part that looks like layering and is not composition: "This delta is targeted to be a delta between two specific commit objects," spending server storage and compute to save client bandwidth. deltas are a transport format between two tree identities; the commit model never sees them.

the pressure is stated against package-based client assembly: "Packages are traditionally composed of partial filesystem trees with metadata and scripts attached, and these are dynamically assembled on the client machine, after a process of dependency resolution. In contrast, OSTree only supports recording and deploying *complete* (bootable) filesystem trees." composition moves to a build or server; delivery becomes whole trees.

## BuildStream: overlay as a construction discipline, tree as the product

BuildStream is the one system here whose composition mechanism is an overlay, and its conflict rule is the opposite of OCI's. "the inputs and output of an element are directory trees," and dependencies are staged into the sandbox root "in deterministic staging order, starting with the basemost elements." when two trees claim one path:

> When 2 elements both have a file at the same path, we say that those files overlap, and staging files which overlap is normally an error.

the escape is per-element and explicit: `overlap-whitelist`, "The overlap whitelist indicates which files this element is allowed to overlap over other elements when staged together with other elements," declared on the depending element, so a later tree may cover an earlier one only where it says so. the error text tells the author which direction resolves it: order the overlapping element above the one it covers.

the product is again one tree. a `stack` element is "a symbolic element used for representing a logical group of elements" with no artifact content of its own, and checking out a toplevel stack yields the merged tree; a `filter` element produces a subset of one parent. the overlay exists during construction, in the sandbox; the artifact that comes out, and that a plugin would commit to OSTree or export as an image, is a single canonical tree.

the pressure, from the project's own description: "BuildStream is a powerful software integration tool that allows developers to automate the integration of software components including operating systems," with freedesktop-sdk and gnome-build-meta as the named users — integration at the scale where hundreds of components must produce one inspectable root.

## OCI: the sequence is the artifact

OCI is the one system that persists the sequence. an image is an ordered array of layers, and the ordering is normative:

> The array MUST have the base layer at index 0. ... Subsequent layers MUST then follow in stack order (i.e. from `layers[0]` to `layers[len(layers)-1]`). ... The final filesystem layout MUST match the result of applying the layers to an empty directory.

identity runs over the sequence: each layer is a descriptor whose digest "acts as a content identifier, enabling content addressability"; the config lists "layer content hashes (`DiffIDs`), in order from first to last"; and "Each image's ID is given by the SHA256 hash of its configuration JSON."

because the model is an append sequence, deletion has to be encoded in-band as data: "A whiteout file is an empty file with a special filename that signifies a path should be deleted," with `.wh.` prefixed names and the opaque `.wh..wh..opq` form that hides all children of a directory. the constraint is explicit: "Whiteout files MUST only apply to resources in lower/parent layers." removing something a lower layer added costs a special file in a higher one, because a sequence has no operation that reaches back.

the tree exists only at unpack. the image is "unpacked into an OCI Runtime Bundle" by tooling the spec does not name; the runtime specification consumes an already-materialized root filesystem and never mentions layers. so the sequence is the durable artifact, content-addressed as a sequence, and the tree is a derived view every consumer rebuilds.

## what a composed system is, according to these five

on the shape: NixOS's toplevel is a tree of symlinks; Guix's system is one store directory; OSTree's commit is a tree with metadata hashed into identity; BuildStream's element output is one tree, with the overlay confined to construction; OCI's image is a sequence. four of the five collapse to a tree somewhere, and the one that does not pays for it with whiteouts.

none of the five composes a system as assertions over a target. assertion-shaped behavior appears in all of them, but at the activation boundary: `switch-to-configuration` reading `/etc/fstab` "and the current systemd status", Guix's reconfigure upgrading stopped-running services at their next stop, OSTree's 3-way /etc merge against the previous deployment. the assertion model is real and it is the activation half's mechanism — M-5b in this project's milestones, fed by an M-5a artifact that is already a value.

on the conflict rule, which cuts across the shape: NixOS discards definitions by numeric priority before a per-type merge that still errors on equal-priority scalar disagreement; Guix refuses statically and pushes resolution to redeclaration outside the fold; CUE, from the configuration note's ground, makes disagreement a lattice bottom and has no override at all; BuildStream fails on overlap unless the depending element whitelists the path; OCI lets extraction order decide silently. the tree systems do not agree among themselves, and neither do the sequence systems. a project choosing a shape still has to choose a rule, and the record beside this note takes that position for the merge operator.

symlinks are load-bearing in the tree systems and ordinary in the sequence one. OSTree puts the symlink target inside the hashed header; NixOS's /etc "consists almost entirely of symlinks"; Guix installs /etc per-entry from the store. OCI carries symlinks as tar entries with no composition semantics, reserving special meaning for whiteouts, which are deliberately plain empty files.

## what this leaves for pith

the artifact M-5a composes is one canonical tree, which is what the kernel's content model already is: tree identity preserving executability and symlink targets is the same construction OSTree arrives at from first principles. a layered format is an export of that tree — a projection from a tree (or a pair of trees) to a sequence — and the whiteout mechanism is the cost the projection pays, not a composition semantic to inherit. assertions over a target belong to M-5b, where the observation and mutation effects exist.

the merge the tree needs is the record-shaped one with a fail-closed conflict rule; [0052](../decisions/0052-the-merge-operator.md) takes its placement and posture. the tree-level overlay — files and directories composing by path — is the same algebra at a different granularity, and the record names it as one mechanism rather than two.

the reading also makes the plan's capture asymmetry concrete: an immutable /etc in the NixOS shape is mostly symlinks into store content, the executor's output-tree capture dereferences symlinks today, and `symlink`/`symlinkat` sit outside the seccomp allowlist. M-5a is where a composed system first needs a `Symlink` entry to survive an action's output.

## questions for the historical pass

- why did NixOS adopt numeric priorities? the papers document the merge and the fixed point and do not argue the priority ladder; the numbers themselves (10, 50, 60, 100, 1000, 1500) look like accreted compatibility.
- which Guix release introduced the extension fold, and what did service composition look like before it — the NEWS trail this pass could not complete.
- how BuildStream stages symlinks inside dependency artifacts; the element documentation read here does not address it.
- how OSTree's 3-way /etc merge behaves on real conflicts; the deployment documentation names the merge and defers its mechanics.

## sources

- [NixOS manual, modularity](https://nixos.org/manual/nixos/stable/#sec-writing-modules)
- [NixOS manual, option types](https://nixos.org/manual/nixos/stable/#sec-option-types)
- [nixpkgs, lib/modules.nix (`mkOverride` and the priority filter)](https://github.com/NixOS/nixpkgs/blob/master/lib/modules.nix)
- [nixpkgs, nixos/modules/system/activation/top-level.nix](https://github.com/NixOS/nixpkgs/blob/master/nixos/modules/system/activation/top-level.nix)
- [NixOS manual, what happens during a system switch](https://nixos.org/manual/nixos/stable/#sec-changing-config)
- [Nix manual, profiles](https://nix.dev/manual/nix/stable/command-ref/files/profiles.html)
- [Dolstra, Löh, Pierron: NixOS: A Purely Functional Linux Distribution (JFP 2010)](https://edolstra.github.io/pubs/nixos-jfp-final.pdf)
- [Dolstra, Hemel: Purely Functional System Configuration Management (HotOS'07)](https://www.usenix.org/event/hotos07/tech/full_papers/dolstra/dolstra.pdf)
- [Guix manual, service composition](https://guix.gnu.org/manual/en/html_node/Service-Composition.html)
- [Guix manual, service types and services](https://guix.gnu.org/manual/en/html_node/Service-Types-and-Services.html)
- [Guix manual, invoking guix system](https://guix.gnu.org/manual/en/html_node/Invoking-guix-system.html)
- [Guix source, gnu/services.scm](https://git.savannah.gnu.org/cgit/guix.git/plain/gnu/services.scm)
- [Guix source, gnu/build/activation.scm](https://cgit.git.savannah.gnu.org/cgit/guix.git/plain/gnu/build/activation.scm)
- [OSTree, repo and object model](https://ostreedevs.github.io/ostree/repo/)
- [OSTree, deployments](https://ostreedevs.github.io/ostree/deployment/)
- [OSTree, atomic upgrades](https://ostreedevs.github.io/ostree/atomic-upgrades/)
- [OSTree, introduction](https://ostreedevs.github.io/ostree/introduction/)
- [OSTree, formats (static deltas)](https://ostreedevs.github.io/ostree/formats/)
- [BuildStream tutorial, overlapping files](https://docs.buildstream.build/2.0/handling-files/overlaps.html)
- [BuildStream, public data (`overlap-whitelist`)](https://docs.buildstream.build/2.0/format_public.html)
- [BuildStream, element dependencies and staging](https://docs.buildstream.build/master/buildstream.element.html)
- [BuildStream, stack elements](https://docs.buildstream.build/master/elements/stack.html)
- [OCI image-spec, manifest](https://github.com/opencontainers/image-spec/blob/main/manifest.md)
- [OCI image-spec, layer filesystem changeset (whiteouts)](https://github.com/opencontainers/image-spec/blob/main/layer.md)
- [OCI image-spec, image config (DiffIDs, image ID)](https://github.com/opencontainers/image-spec/blob/main/config.md)
- [OCI runtime-spec, bundles](https://github.com/opencontainers/runtime-spec/blob/main/bundle.md)
