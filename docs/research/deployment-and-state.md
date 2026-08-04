---
schema: design-doc/v1
id: research-deployment-and-state
title: desired state and deployment
summary: the lineages behind Terraform, Kubernetes reconciliation, Pulumi identity, and Crossplane, and what each one cost
kind: research
status: researching
evidence: reviewed
created: 2026-03-10
updated: 2026-03-10
tags:
  - research
  - deployment
  - state
relations:
  informed_by: []
  depends_on:
    - research-method
  supersedes: []
---

# desired state and deployment

build actions are easy to trust. declared inputs, sandbox, content-addressed output. re-running a build produces the same bytes or surfaces a reason.

deployment does not share this property. making a database reach a given state is not a build action. the database exists outside the graph, changes between observations, is touched by other actors, and if a change fails partway there is often no clean state to return to.

every tool that has tried to make deployment look like configuration has had to absorb this, and each has paid for it somewhere specific. this note traces four systems and names the cost.

## the pressure

operators of large fleets passed the scale where anyone could make changes by hand. they needed a way to write down the intended state of machines, networks, databases, and services, and a way to move reality toward that description without scripting every step.

the configuration-management answer, from CFEngine through Ansible, was to push ordered tasks and rely on module authors for idempotence. it works. the idempotence lives in each module, which is the weakness.

the infrastructure-provisioning answer, Terraform, was to declare resources and let a planner derive create, update, and destroy calls.

the control-plane answer, Kubernetes, was to run continuous loops that converge observed state to a desired spec.

each refused to give up something different, and the cost is the part worth recording.

## Terraform: the plan, the state file, and the missing transaction

Terraform's surface is declarative. resources in HCL, a plan derived from a diff against observed state, the plan reviewed, then applied. the user-facing property is that changes are inspectable before they happen.

the mechanism underneath is less clean. Terraform keeps a state file binding resource addresses in configuration (`aws_db_instance.primary`) to remote objects (`db-abc123`), with enough metadata to decide the next operation. the state file is not a cache that can be rebuilt from configuration. it is the record of which remote objects this configuration claims to own.

the documented failure modes share a root cause: there is no transaction. an apply that fails halfway leaves some resources changed and others not. the state file reflects whatever was written before the failure, which is some third state that is neither the old world nor the new one. there is no rollback. the recovery pattern HashiCorp and the community converged on is roll-forward: fix the cause, plan again, apply again. this is workable for stateless resources and hostile to stateful ones.

two specific failure modes recur in any design that copies this shape. the first is the `provider produced inconsistent result after apply` error, documented with its own HashiCorp support article. a provider creates or updates a resource during apply, the post-apply refresh cannot find it, and Terraform holds a state entry for an object that may or may not exist. the next plan is built on that uncertainty. the second is drift: the state file is a snapshot, so anything that changes the remote object out of band (other tools, other engineers, the cloud itself, time) is invisible until the next refresh, and refresh is itself a fallible observation.

Terraform's invariant is the inspectable plan and the explicit ownership binding. the cost is treating a snapshot of observations as durable truth, with no transactional story for partial failure.

## Kubernetes: throw away the plan

Kubernetes made the opposite call on nearly every axis.

a controller is a non-terminating loop. it compares a desired spec against observed state and acts to reduce the difference. there is no plan artifact and no terminal state. the property Kubernetes refused to give up is level triggering: action is re-derived from current observed state on every pass, so a missed event, a restarted controller, or a briefly-unreachable API does not corrupt the outcome. the next pass recomputes. this is why the cluster can be built from many small controllers that fail and restart without becoming unrecoverable.

the cost is symmetric with Terraform's gain. there is no durable plan. there is no point at which a controller can promise "here is exactly what will happen next and nothing more," because the world may move between the claim and the action. `kubectl apply --dry-run=server` is a hint, not a contract. the absence of an inspectable plan is the most-cited reason teams run Terraform or Pulumi alongside Kubernetes rather than driving everything through controllers.

Kubernetes also made a quiet choice about identity. controllers reconcile on labels and `metadata.uid`. an object deleted and recreated by the platform gains a new uid, and whether that counts as the same object is a question Kubernetes declines to answer. controllers care about the set of objects matching a label selector. individual object continuity is not modeled.

## Pulumi: stable identity, source-derived

Pulumi's contribution is narrow and load-bearing for this project's identity question.

Terraform binds configuration addresses to remote objects through the state file, so the binding is only as stable as the state file. Pulumi introduced resource URNs: stable, globally-unique identifiers derived from a resource's logical position in the program (project, stack, parent, type, name), for example `urn:pulumi:prod::myapp::aws:ec2/instance:Instance::web-server`.

a URN survives the cloud re-creating the underlying object, because it is keyed off the program rather than the cloud. it survives moving code between files in most cases. it does not survive a rename that changes the logical path, which is why Pulumi provides an explicit `aliases` mechanism to carry identity across the rename.

the invariant is trackability across renames and refactors. the cost is the same as any source-derived identity: a silent refactor breaks continuity, and the escape hatch only works if the author knows to use it. Pulumi collapsed identity to one source-derived string. decision 0005 separates identity into four types precisely to avoid that collapse, and decision 0013 asks whether four is enough for deployment.

## Crossplane: what happens when a design tries to inherit both parents

Crossplane is the most relevant prior attempt at the synthesis requirement S-8 reaches for. it puts infrastructure resources inside Kubernetes as custom resources and runs provider controllers that reconcile them against external clouds. its own framing is control-plane versus CLI: Terraform is a one-shot process that only reconciles when invoked, Crossplane is a set of always-on control loops that reconcile continuously.

the appeal is Kubernetes' drift handling applied to infrastructure. an out-of-band change gets reverted on the next pass.

the costs are the costs of both parents plus a new one. from Kubernetes it inherits the absence of a plan preview, the single most-cited operational limitation in accounts from teams that otherwise adopted it. from Terraform it inherits the difficulty of modeling resources that cannot be freely destroyed and recreated. the new cost is that continuous reconciliation of many resources loads the Kubernetes API server and the clouds, and the provider ecosystem is younger and less uniform than Terraform's.

Crossplane did not unify plan-based and reconciliation-based execution. it chose reconciliation and learned to live without plans. S-8's claim that one-shot application and continuous reconciliation "can execute the same desired-state and transition semantics" is stronger than what Crossplane achieved. it is achievable only if both modes share an abstraction underneath, which is the subject of decision 0012.

## what this project should inherit

the inspectable plan from Terraform, without the state-as-truth treatment and without the missing transactions.

level triggering from Kubernetes as a robustness property of the reconciliation mode, without the absence of a plan and without the assumption that an always-running control plane is the only execution model.

the notion of stable logical identity from Pulumi, without collapsing it into one source-derived string.

the synthesis none of the four reached is a plan that is a real value, pinned to the observation revisions it was built from, with execution that validates those pins at apply time and re-derives when they break. that abstraction is what lets one-shot and continuous share semantics. Crossplane showed the cost of choosing only one mode. Terraform showed the cost of treating observations as durable. the missing piece is the abstraction that contains both.

## questions for the historical pass

- why did Terraform's designers choose a state file over re-deriving bindings from configuration, and which of those reasons still hold under content-addressed storage?
- what did the early move from edge-triggered to level-triggered reconciliation in Kubernetes actually fix, and which of those fixes depend on an always-running control plane?
- how does Pulumi's aliasing fail in practice, and what would a design that does not need explicit aliasing give up?
- how do Crossplane providers handle resources that cannot be reconciled to a desired state without destructive operations, and is there a general pattern?
- which of Terraform's failure modes are inherent to plan-and-apply, and which are accidents of the state-file implementation?

## sources

- [Terraform language](https://developer.hashicorp.com/terraform/language)
- [Terraform state purpose](https://developer.hashicorp.com/terraform/language/state/purpose)
- [Terraform providers](https://developer.hashicorp.com/terraform/language/providers)
- [HashiCorp: provider produced inconsistent result after apply](https://support.hashicorp.com/hc/en-us/articles/1500006254562-Provider-produced-inconsistent-result-after-apply-Root-resource-was-present-but-now-absent)
- [HashiCorp discuss: apply previous state file or undo plan](https://discuss.hashicorp.com/t/apply-previous-state-file-or-undo-plan/57833)
- [When Terraform apply fails halfway through](https://encore.dev/articles/terraform-apply-fails)
- [Kubernetes controller pattern](https://kubernetes.io/docs/concepts/architecture/controller/)
- [Level triggering and reconciliation in Kubernetes](https://hackernoon.com/level-triggering-and-reconciliation-in-kubernetes-1f17fe30333d)
- [Pulumi vs Terraform](https://www.pulumi.com/docs/iac/comparisons/terraform/)
- [Crossplane vs Terraform](https://blog.crossplane.io/crossplane-vs-terraform/)
- [Crossplane is great, but what about critical infrastructure](https://www.eficode.com/insights/blog/crossplane-is-great-but-what-about-critical-infrastructure)
- [Terraform vs Pulumi vs Crossplane](https://platformengineering.org/blog/terraform-vs-pulumi-vs-crossplane-iac-tool)
