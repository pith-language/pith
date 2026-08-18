---
schema: design-doc/v1
title: extension interfaces
summary: what Bazel, Buck2, Terraform, Kubernetes, PostgreSQL, and Nix let outside code define, where each draws the boundary, and what each offers as evidence that an extension is not second class
id: research-extension-interfaces
kind: research
status: researching
evidence: preliminary
created: 2026-08-15
updated: 2026-08-18
tags:
  - research
  - libraries
  - kernel
relations:
  informed_by:
    - research-build-systems
    - research-nix
  depends_on:
    - research-method
  supersedes: []
---

# extension interfaces

a system that ships its own domain libraries and also invites outside ones is answering two questions at once. can an extension reach what the built-ins reach? can an extension damage what it should not? call the first parity and the second isolation. the five systems below answer one or the other, and the choice of mechanism is what decides which.

pith's requirement U-10 asks for evidence and not an assurance: "tests prove that an external library can replace or extend it without hidden hooks." so the reading below also asks what each system offers a skeptic.

## Bazel: rules in Starlark, and built-in rules in java until 2024

Bazel's extension surface is a language: "Bazel provides an extensibility model for writing rules using the Starlark language. These rules are written in `.bzl` files, which can be loaded directly from `BUILD` files." a rule "defines a series of actions that Bazel performs on inputs to produce a set of outputs, which are referenced in providers returned by the rule's implementation function," and it is created by calling `rule` with a set of attributes and an implementation function. the documented purpose is a new domain: "By defining your own rules, you can add support for languages and tools that Bazel doesn't support natively."

a rule is the powerful construct and the documentation says so: "A rule is more powerful than a macro. It can access Bazel internals and have full control over what is going on."

the surface existed long before Bazel's own rules used it. in november 2020 Lukács T. Berki announced on bazel-discuss the plan to rewrite the native java-implemented rules in Starlark and inject them into `BUILD` files, which avoided both a `load()` statement everywhere and the version skew that separate repositories would bring. the migration shipped in Bazel 8.0 in december 2024: "All `java_*` rules now reside in rules_java", "All `py_*` rules and providers (like PyInfo) have been moved to rules_python", "`*_proto_library` rules have been moved to protobuf", "All `sh_*` rules are now part of rules_shell", with the android rules and the c++ toolchain symbols moving the same way. four years passed between the plan and the release, and Bazel described its extension model in the same terms throughout.

one privilege survived the move. asked in that thread what private interface remained between Bazel and its c++ rules, Jon Brandvein answered that "there's no internal API between Bazel and rules_cc, aside from one secret handshake tag that tells Bazel not to complain about rules_cc instantiating the native rules."

## Buck2: no rules in the binary

Buck2 shipped with the property Bazel spent four years reaching. "All Buck2 rules are written in Starlark - whereas, in Buck1, they were written in Java as part of the binary, which makes iteration on rules much faster." its documentation states the consequence for extensions: "The Buck2 binary is entirely language agnostic - as a consequence of having all the rules external to the binary, the most important and complex rule (such as in C++), don't have access to magic internal features."

the shipped rules live in a prelude that the rule-authoring guide tells new rules to stay out of: "The only advantage of the `prelude` is that rules can be used without a corresponding `load`, which is generally considered a misfeature... If your rule is not already in Buck1, then you can define it wherever you like, with a preference for it not being in `fbcode/buck2/prelude`." a build configured without the prelude has no `genrule` and no `sh_binary`, since those are prelude rules like any other.

## Terraform: providers in another process, behind a versioned protocol

Terraform draws the boundary at the process. "Terraform Plugins are written in Go and are executable binaries invoked by Terraform Core over RPC. Each plugin exposes an implementation for a specific service, such as AWS, or provisioner, such as bash." plugins are "executed as a separate process and communicate with the main Terraform binary over an RPC interface," and core "uses remote procedure calls (RPC) to communicate with Terraform Plugins, and offers multiple ways to discover and load plugins to use."

the wire between them is a versioned compatibility surface: "The Terraform plugin protocol is a versioned interface between Terraform CLI and Terraform Plugins," carried over protocol buffers and gRPC, where "Major versions of the protocol delineate Terraform CLI and Terraform Plugin compatibility. Minor versions of the protocol are additive." protocol 6 is compatible with Terraform CLI 1.0 and later, protocol 5 with 0.12 and later.

the same documentation divides the work. core's responsibilities are "reading and interpolating configuration files and modules; Resource state management; Construction of the Resource Graph; Plan execution; Communication with plugins over RPC." a provider's are to initialize its libraries, authenticate, and "Define managed resources and data sources that map to specific services." a provider can declare any resource it likes. the graph, the plan, and the state model are core's, and no provider extends them.

## Kubernetes: extension as an api resource

Kubernetes puts the extension point in its api. "A custom resource is an extension of the Kubernetes API that is not necessarily available in a default Kubernetes installation," where a resource in general "is an endpoint in the Kubernetes API that stores a collection of API objects of a certain kind; for example, the built-in pods resource contains a collection of Pod objects." defining a `CustomResourceDefinition` "creates a new custom resource with a name and schema that you specify. The Kubernetes API serves and handles the storage of your custom resource," which "frees you from writing your own API server to handle the custom resource."

clients cannot tell the two apart: "Once a custom resource is installed, users can create and access its objects using kubectl, just as they do for built-in resources like Pods," and the documentation names viewing new types "in a Kubernetes UI, such as dashboard, alongside built-in types" among the reasons to define one. storage, schema validation, versioning, and the tooling surface are shared.

behaviour is not. a `CustomResourceDefinition` supplies storage and an endpoint; what the object does is a controller its author writes and runs, while a Pod's scheduling and lifecycle are implemented in the control plane.

## PostgreSQL: extension through the catalogs

PostgreSQL's manual explains extensibility as a property of the implementation: "PostgreSQL is extensible because its operation is catalog-driven." it draws the comparison itself: "One key difference between PostgreSQL and standard relational database systems is that PostgreSQL stores much more information in its catalogs: not only information about tables and columns, but also information about data types, functions, access methods, and so on... By comparison, conventional database systems can only be extended by changing hardcoded procedures in the source code or by loading modules specially written by the DBMS vendor." the server reads those tables at run time, so "these tables can be modified by the user, and since PostgreSQL bases its operation on these tables, this means that PostgreSQL can be extended by users."

the code an extension supplies runs inside the server: "The PostgreSQL server can moreover incorporate user-written code into itself through dynamic loading. That is, the user can specify an object code file (e.g., a shared library) that implements a new type or function, and PostgreSQL will load it as required." a shared library in the server has the server's authority. the built-in types and functions are catalog entries on the same terms as an extension's.

## Nix: one primitive, and nixpkgs above it

the Nix language has a single function that reaches outside evaluation: "The most important built-in function is `derivation`, which is used to describe a single derivation: a specification for running an executable on precisely defined input files to repeatably produce output files at uniquely determined file system paths." stdenv, the build phases, and the language-specific builders are library code written above it, in the language any third-party expression uses. nixpkgs is larger and better known than other expressions and has no channel to the evaluator they lack.

## where the five disagree

they agree that outside code should be able to define a new kind of thing. they disagree about which of the two questions the mechanism answers.

Terraform and Kubernetes answer isolation. a provider is a separate process reachable only over a versioned protocol, and a custom resource is served by an api server that stores it without executing anything the author wrote. both pay in shape: the protocol and the api fix what an extension can be, so Terraform's resource graph and plan and Kubernetes' scheduling stay where they are.

Buck2, PostgreSQL, and Nix answer parity, by routing the built-ins through the extension surface. rules are external to the buck2 binary, built-in types are catalog rows, and stdenv is an expression calling `derivation`. all three accept that an extension runs with the system's own authority; PostgreSQL says so in the manual.

the second disagreement is about evidence, and it is where the field is thin. Buck2, PostgreSQL, and Nix can point at their own construction. Bazel could not until the migration shipped, and the four years are the useful part of the record: the surface was public and the documentation was unchanged while the rules were still java. Kubernetes points at a client-visible property. none of the five has a test that fails when an extension becomes second class, and Bazel's exemption tag is the kind of thing such a test would catch.

## questions for this project

pith reaches parity the way Buck2 and PostgreSQL do. a domain is a rust crate that registers rules on an engine, and 0009's peer claim and U-10's no-privilege requirement are that same property stated as requirements. the argument the workspace has been making is Bazel's: the first-party domains use public interfaces, so an outside one could. Bazel's four years are the reason to want something stronger.

what pith can build and none of the five has is the test. a domain crate that registers through the public surface, depends on no other domain, and appears nowhere in the kernel makes the claim something CI can fail. it proves less than Buck2's construction, since pith's kernel does hold rule-shaped things the domains do not and the first-party crates compile into the same workspace. it proves more than a sentence in a document, because a hook added for a first-party domain breaks it.

isolation stays where pith already put it: in the action contract and the executor's confinement, and nowhere in the domain boundary. Nix splits the same way, between an expression that may compute anything and a builder that runs under the daemon's rules.

## sources

- [Bazel: rules](https://bazel.build/extending/rules)
- [Bazel: macros and rules concepts](https://bazel.build/extending/concepts)
- [bazel-discuss: next steps for Starlarkifying native rules (18 November 2020)](https://groups.google.com/g/bazel-discuss/c/XNvpWcge4AE)
- [Bazel 8.0 LTS release announcement (9 December 2024)](https://blog.bazel.build/2024/12/09/bazel-8-release.html)
- [Buck2: why buck2](https://buck2.build/docs/about/why/)
- [Buck2: writing rules](https://buck2.build/docs/rule_authors/writing_rules/)
- [Terraform: how Terraform works with plugins](https://developer.hashicorp.com/terraform/plugin/how-terraform-works)
- [Terraform: the plugin protocol](https://developer.hashicorp.com/terraform/plugin/terraform-plugin-protocol)
- [Kubernetes: custom resources](https://kubernetes.io/docs/concepts/extend-kubernetes/api-extension/custom-resources/)
- [PostgreSQL: how extensibility works](https://www.postgresql.org/docs/current/extend-how.html)
- [Nix reference manual: derivations](https://nix.dev/manual/nix/2.24/language/derivations)
