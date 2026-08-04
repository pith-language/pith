---
schema: design-doc/v1
id: decision-0006-linux-first
title: target Linux first without putting Linux in the kernel
summary: use Linux for the first complete vertical slice while keeping its concepts in libraries and adapters
kind: decision
status: proposed
created: 2026-03-26
updated: 2026-03-26
tags:
  - linux
  - portability
relations:
  informed_by:
    - research-nix
  depends_on:
    - decision-0001-generic-kernel
  supersedes: []
---

# target Linux first without putting Linux in the kernel

## context

a generic design needs a real target to expose missing semantics. Linux provides builds, filesystems, services, boot, hardware, secrets, and deployment in one environment.

making the kernel portable by avoiding every platform-specific idea can also produce abstractions too weak to express a real system.

## proposed decision

the first complete implementation targets Linux. Linux concepts live in a first-party system library and adapters.

the kernel has no built-in Unix path, user, permission, process, signal, mount, or init-system types.

## alternatives considered

### Linux-specific core

the engine could directly model filesystem permissions, processes, users, and systemd units.

this shortens the path to a working operating-system manager. it prevents non-Linux domains from using the core without inheriting irrelevant semantics.

### several platforms from the start

the project could require Linux, macOS, and Windows implementations before settling interfaces.

this would reveal portability issues early and multiply the implementation work before the kernel is proven.

### platform-neutral abstractions only

the first system library could expose only concepts shared by every platform.

this tends toward a weak lowest common denominator. platform-specific capabilities should be expressible through typed extensions instead.

## unresolved

the first runtime and init adapter have not been selected. choosing systemd would be practical for Linux without making it the semantic definition of a service.

