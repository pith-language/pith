//! The set of in-flight evaluation chains (decision 0022).
//!
//! A *chain* is one depth-first stack of [`EvalFrame`]s — exactly what the
//! engine drove before this module existed. What is new is that there can be
//! more than one at a time: [`PureStep::NeedAll`](super::PureStep::NeedAll)
//! opens a fan-out *group* whose requests each become their own chain, and
//! [`Engine::run_many`](super::Engine::run_many) starts one chain per root
//! request.
//!
//! Chains are the unit of concurrency. A chain runs synchronously, on the same
//! step machine as before, until it completes or parks — on a fan-out group, or
//! on an action whose executor has not returned. Only parked chains yield, so
//! the invariant the driver relies on (a chain's stack does not change under
//! it) holds by construction: a parked chain is not stepped, and a running
//! chain never awaits.
//!
//! This module owns chain bookkeeping only. It never touches the arena, so it
//! can be reasoned about without the graph.

use std::collections::VecDeque;

use pith_core::{Interface, Pure, Request, RuleId, Value};
use pith_diag::PithResult;

use super::diagnostics::{InternalInvariant, internal_diag};
use super::ir::{EvalFrame, Evaluation, Resumption};

pub(super) type ChainId = usize;
pub(super) type GroupId = usize;

/// Where a chain's final value goes.
enum ChainSink {
    /// A root request; the value is result `index` of the run.
    Root(usize),
    /// One request of a fan-out group.
    Group { group: GroupId, slot: usize },
}

struct Chain {
    stack: Vec<EvalFrame>,
    sink: ChainSink,
}

/// One `NeedAll` in flight: the frame that opened it waits until every slot is
/// filled, then resumes with all of them at once.
struct Group {
    parent: ChainId,
    slots: Box<[Option<Value>]>,
    remaining: usize,
}

pub(super) struct Scheduler {
    /// Slots are tombstoned rather than removed so a [`ChainId`] stays valid
    /// for the life of the run; parked chains are referenced by in-flight
    /// actions that outlive several scheduler turns.
    chains: Vec<Option<Chain>>,
    groups: Vec<Option<Group>>,
    ready: VecDeque<ChainId>,
    roots: Vec<Option<Evaluation>>,
}

impl Scheduler {
    /// Start one ready chain per root frame, in order.
    pub(super) fn with_roots(roots: Vec<EvalFrame>) -> Self {
        let count = roots.len();
        let chains = roots
            .into_iter()
            .enumerate()
            .map(|(index, frame)| {
                Some(Chain {
                    stack: vec![frame],
                    sink: ChainSink::Root(index),
                })
            })
            .collect();
        Self {
            chains,
            groups: Vec::new(),
            ready: (0..count).collect(),
            roots: (0..count).map(|_| None).collect(),
        }
    }

    pub(super) fn next_ready(&mut self) -> Option<ChainId> {
        self.ready.pop_front()
    }

    /// The frame `chain` is currently evaluating — the one that requested
    /// whatever the chain is about to do.
    pub(super) fn top(&self, chain: ChainId) -> PithResult<&EvalFrame> {
        match self
            .chains
            .get(chain)
            .and_then(Option::as_ref)
            .and_then(|chain| chain.stack.last())
        {
            Some(frame) => Ok(frame),
            None => Err(internal_diag(InternalInvariant::PureLostRequestingFrame)),
        }
    }

    pub(super) fn stack_mut(&mut self, chain: ChainId) -> PithResult<&mut Vec<EvalFrame>> {
        match self.chains.get_mut(chain).and_then(Option::as_mut) {
            Some(chain) => Ok(&mut chain.stack),
            None => Err(internal_diag(InternalInvariant::SchedulerLostChain)),
        }
    }

    /// Make `chain` runnable again. Private because a chain must never become
    /// ready without a resumption waiting for its top frame; [`Self::resume`]
    /// is the only way to arrange both.
    fn make_ready(&mut self, chain: ChainId) {
        self.ready.push_back(chain);
    }

    /// Set the resumption for the top frame of `chain` and make it runnable.
    pub(super) fn resume(&mut self, chain: ChainId, resumption: Resumption) -> PithResult<()> {
        let Some(frame) = self.stack_mut(chain)?.last_mut() else {
            return Err(internal_diag(InternalInvariant::SchedulerLostFanOutFrame));
        };
        frame.resume_with = Some(resumption);
        self.make_ready(chain);
        Ok(())
    }

    /// Open a fan-out group of `count` slots on behalf of `parent`, which stays
    /// parked until every slot is filled.
    pub(super) fn open_group(&mut self, parent: ChainId, count: usize) -> GroupId {
        let group = self.groups.len();
        self.groups.push(Some(Group {
            parent,
            slots: (0..count).map(|_| None).collect(),
            remaining: count,
        }));
        group
    }

    /// Start a chain that will fill `slot` of `group`.
    pub(super) fn start_group_chain(
        &mut self,
        group: GroupId,
        slot: usize,
        frame: EvalFrame,
    ) -> ChainId {
        let chain = self.chains.len();
        self.chains.push(Some(Chain {
            stack: vec![frame],
            sink: ChainSink::Group { group, slot },
        }));
        self.ready.push_back(chain);
        chain
    }

    /// Fill a slot whose value was already known — a reused result needs no
    /// chain of its own. Resumes the parent if this was the last slot.
    pub(super) fn fill_group_slot(
        &mut self,
        group: GroupId,
        slot: usize,
        value: Value,
    ) -> PithResult<()> {
        self.record_slot(group, slot, value)?;
        self.resume_group_if_complete(group)
    }

    /// Retire a chain that reached its final value, delivering it to the chain's
    /// sink: a root result, or a slot of the group that spawned it.
    pub(super) fn complete_chain(
        &mut self,
        chain: ChainId,
        evaluation: Evaluation,
    ) -> PithResult<()> {
        let Some(retired) = self.chains.get_mut(chain).and_then(Option::take) else {
            return Err(internal_diag(InternalInvariant::SchedulerLostChain));
        };
        match retired.sink {
            ChainSink::Root(index) => {
                let Some(slot) = self.roots.get_mut(index) else {
                    return Err(internal_diag(InternalInvariant::SchedulerLostRootSlot));
                };
                *slot = Some(evaluation);
                Ok(())
            }
            ChainSink::Group { group, slot } => {
                self.record_slot(group, slot, evaluation.value)?;
                self.resume_group_if_complete(group)
            }
        }
    }

    fn record_slot(&mut self, group: GroupId, slot: usize, value: Value) -> PithResult<()> {
        let Some(group) = self.groups.get_mut(group).and_then(Option::as_mut) else {
            return Err(internal_diag(InternalInvariant::SchedulerLostGroup));
        };
        let Some(cell) = group.slots.get_mut(slot) else {
            return Err(internal_diag(InternalInvariant::SchedulerLostGroup));
        };
        if cell.replace(value).is_none() {
            group.remaining = group.remaining.saturating_sub(1);
        }
        Ok(())
    }

    fn resume_group_if_complete(&mut self, group: GroupId) -> PithResult<()> {
        let Some(pending) = self.groups.get_mut(group).and_then(Option::as_mut) else {
            return Err(internal_diag(InternalInvariant::SchedulerLostGroup));
        };
        if pending.remaining > 0 {
            return Ok(());
        }
        let parent = pending.parent;
        let Some(retired) = self.groups.get_mut(group).and_then(Option::take) else {
            return Err(internal_diag(InternalInvariant::SchedulerLostGroup));
        };
        let mut values = Vec::with_capacity(retired.slots.len());
        for slot in retired.slots.into_vec() {
            let Some(value) = slot else {
                return Err(internal_diag(InternalInvariant::SchedulerLostGroup));
            };
            values.push(value);
        }
        self.resume(parent, Resumption::Many(values.into_boxed_slice()))
    }

    /// The labels of the frames that would form a cycle if `child` were
    /// evaluated under `chain`, or `None` if there is no cycle.
    ///
    /// The scope is the chain's own stack plus every ancestor chain's, because
    /// a fan-out child is evaluated *within* the frame that requested it: a
    /// request that reappears below a `NeedAll` is as circular as one that
    /// reappears below a `Need`.
    pub(super) fn cycle_chain(
        &self,
        chain: ChainId,
        child_rule: RuleId,
        child_interface: &Interface,
        child_inputs: &[Value],
        child_label: &str,
    ) -> Option<Vec<Box<str>>> {
        let scope = self.scope(chain);
        let repeats = |frame: &&EvalFrame| {
            frame.rule == child_rule
                && frame.request.interface == *child_interface
                && *frame.request.inputs == *child_inputs
        };
        if !scope.iter().any(repeats) {
            return None;
        }
        let mut labels: Vec<Box<str>> = scope
            .iter()
            .map(|frame| frame.request.label.clone())
            .collect();
        labels.push(child_label.into());
        Some(labels)
    }

    /// Every frame in scope for `chain`, outermost first: the ancestor chains'
    /// stacks in root-to-leaf order, then the chain's own.
    ///
    /// The walk cannot stop short of a root: a chain's parent is parked until
    /// the group completes, a group is retired only when every slot is filled,
    /// and a chain is retired only when it is driven — which a parked chain is
    /// not. So while any child is live, every ancestor of it is too.
    fn scope(&self, chain: ChainId) -> Vec<&EvalFrame> {
        let mut lineage = vec![chain];
        let mut current = chain;
        while let Some(parent) = self.parent_of(current) {
            lineage.push(parent);
            current = parent;
        }
        lineage.reverse();
        lineage
            .into_iter()
            .filter_map(|id| self.chains.get(id).and_then(Option::as_ref))
            .flat_map(|chain| chain.stack.iter())
            .collect()
    }

    fn parent_of(&self, chain: ChainId) -> Option<ChainId> {
        match self.chains.get(chain).and_then(Option::as_ref)?.sink {
            ChainSink::Root(_) => None,
            ChainSink::Group { group, .. } => {
                Some(self.groups.get(group).and_then(Option::as_ref)?.parent)
            }
        }
    }

    /// Every frame the scheduler still holds, across all live chains. The
    /// driver uses this to fail whatever was still pending when a run aborted.
    pub(super) fn live_frames(&self) -> impl Iterator<Item = &EvalFrame> {
        self.chains
            .iter()
            .filter_map(Option::as_ref)
            .flat_map(|chain| chain.stack.iter())
    }

    /// The root evaluations, in request order. `None` for any root that did not
    /// finish, which only happens on an aborted run.
    pub(super) fn into_roots(self) -> Vec<Option<Evaluation>> {
        self.roots
    }
}

/// The `Request<Pure>` fields [`Scheduler::cycle_chain`] compares. Kept as a
/// helper so the two call sites (`Need` and `NeedAll`) cannot drift apart in
/// what counts as the same request.
pub(super) fn cycle_key(request: &Request<Pure>) -> (&Interface, &[Value], &str) {
    (&request.interface, &request.inputs, &request.label)
}
