---
schema: design-doc/v1
id: design-first-party-domains
title: first-party domains
summary: the build, package, environment, service, system, deployment, secrets, and policy libraries shipped without private engine hooks
kind: design
status: proposed
created: 2026-04-09
updated: 2026-04-09
tags:
  - libraries
  - product
relations:
  informed_by:
    - research-nix
  depends_on:
    - decision-0004-first-party-without-privilege
    - decision-0009-peer-first-party-domains
    - design-kernel
  supersedes: []
---

# first-party domains

the official distribution ships libraries for builds, packages, development environments, services, systems, deployments, secrets, and policy.

these domains are peers. a build does not need a system definition. a package does not need a deployment. a development environment is not a fake production machine.

the values still compose. a build output can become a package payload. a package can become one part of a system. a service can be realized on a machine or another runtime. a deployment can compare any supported desired value with observations from an adapter.

## build

the build library defines sources, targets, toolchains, actions, tests, checks, and artifacts over kernel rules and content storage.

## package

the package library adds semantic package identity, versions, variants, dependency constraints, locks, distribution, and resolution explanations.

## development environments

the environment library composes tools, packages, variables, commands, and local services without mutating global host configuration.

## services

the services library models long-running processes, supervisors, and service-level contracts as values a system or deployment can own and observe. *to be written.*

## system management

the system library defines filesystems, users, services, mounts, devices, networking, boot configuration, persistent data, and operating-system composition.

Linux is the first target. systemd and other Linux mechanisms remain adapter choices.

## deployment

the deployment library combines desired values, observations, ownership, transition constraints, and mutation capabilities. it supports plan inspection, one-shot application, and later continuous reconciliation through the same model.

## secrets

the secrets library models secret references, consumers, rotation, and capability-grained access without passing secret bytes through ordinary configuration. *to be written.*

## policy

the policy library expresses authorization, admission, and composition constraints as inspectable values rather than ambient rules. *to be written.*

## the extension test

an external library should be able to replace the official package model or define a new domain without changing the kernel. an external implementation should also be able to use caching, remote execution, provenance, and queries on the same terms as official code.

