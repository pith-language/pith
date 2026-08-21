//! A run's declared ceiling (decision 0059).
//!
//! The bound is the one mechanism five callers were each deferring to the next
//! milestone: a wall clock and a step budget over a run, and the wall clock
//! again over each action the run starts. It is declared by the caller rather
//! than defaulted — no number is both generous enough for a real build and
//! small enough to stop a runaway — and a run without one is unbounded, as
//! every run was before this module existed.

use std::time::Instant;

/// The wall-clock deadline and step budget one run runs under.
///
/// The deadline is polled at the engine's scheduling boundaries and handed to
/// every action the run starts, so the executor that holds a child enforces it
/// while the driver is asleep waiting for that child. The step budget is spent
/// one pure step at a time, which is what bounds a body that yields an
/// unbounded sequence of distinct requests — a shape cycle detection cannot
/// refuse, because no request repeats.
///
/// The bound is authority for an execution, not content of a request: it does
/// not participate in any computation key, and an attempt recorded under a
/// larger bound may be served to a smaller one, because reuse serves a
/// completed attempt without running anything.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct RunBound {
    deadline: Option<Instant>,
    step_budget: Option<u64>,
}

impl RunBound {
    /// The bound of a run that has neither ceiling: unbounded, as runs were
    /// before this type existed.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            deadline: None,
            step_budget: None,
        }
    }

    /// Stop the run at `deadline` (and every action it starts).
    #[must_use]
    pub const fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Stop the run after `steps` pure steps, across every chain it opens. A
    /// budget of zero is already exhausted.
    #[must_use]
    pub const fn with_step_budget(mut self, steps: u64) -> Self {
        self.step_budget = Some(steps);
        self
    }

    /// Whether the wall clock has passed this bound's deadline. A bound
    /// without one is never exceeded on the clock.
    pub(super) fn deadline_exceeded(self) -> bool {
        self.deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
    }

    /// The deadline every action this run starts inherits, when it has one.
    pub(super) fn action_deadline(self) -> Option<Instant> {
        self.deadline
    }

    /// The step half of this bound, ready to be spent.
    pub(super) fn step_budget(self) -> StepBudget {
        StepBudget {
            remaining: self.step_budget,
            total: self.step_budget,
        }
    }
}

/// The step half of a [`RunBound`], consumed one pure step at a time.
///
/// Spent inside the step machine rather than at scheduling boundaries because
/// a body that yields fresh requests forever steps many times between two
/// boundaries.
#[derive(Debug)]
pub(super) struct StepBudget {
    remaining: Option<u64>,
    total: Option<u64>,
}

impl StepBudget {
    /// The budget of an unbounded run.
    pub(super) const fn unbounded() -> Self {
        Self {
            remaining: None,
            total: None,
        }
    }

    /// Spend one step, reporting whether the run may keep stepping.
    pub(super) fn spend(&mut self) -> bool {
        match self.remaining.as_mut() {
            None => true,
            Some(remaining) => match remaining.checked_sub(1) {
                Some(next) => {
                    *remaining = next;
                    true
                }
                None => false,
            },
        }
    }

    /// The declared total, for the diagnostic that names what ran out.
    pub(super) fn total(&self) -> Option<u64> {
        self.total
    }
}
