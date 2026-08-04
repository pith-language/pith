---
schema: design-doc/v1
id: decision-0009-peer-first-party-domains
title: keep first-party domains as peers
summary: builds, packages, development environments, services, system management, and deployments compose without one becoming the universal parent model
kind: decision
status: accepted
created: 2026-03-31
updated: 2026-03-31
tags:
  - scope
  - libraries
relations:
  informed_by:
    - research-nix
  depends_on:
    - decision-0001-generic-kernel
    - decision-0004-first-party-without-privilege
  supersedes: []
---

# keep first-party domains as peers

## context

describing the project as a system compiler made builds and packages look like internal stages of operating-system construction.

describing it only as a build system would create the opposite problem. live state and deployment would become scripts attached after the real model ends.

## decision

the official distribution supports builds, packages, development environments, services, system management, and deployments as peer libraries.

their values compose through the kernel. none is required merely to use another.

## alternatives considered

### system management as the root domain

all outputs eventually become part of a desired machine or fleet.

this matches NixOS-shaped use cases and weakens standalone builds, libraries, data pipelines, and application deployment.

### build system as the root domain

everything could be represented as an artifact-producing build target.

this gives one graph. observations, ownership, mutation, freshness, and partial external failure do not have build-action semantics.

### one universal resource model

build targets, packages, services, and deployments could all become generic resources with lifecycle methods.

this creates a uniform API by erasing useful distinctions. caching a compiler invocation and retrying a cloud mutation require different contracts.

## consequences

the product has several useful entry points over one semantic engine.

cross-domain operations need explicit conversions. a build output does not silently become a package, and a package does not silently become a deployed service.

