//! Evaluation state for the pure-rule interpreter: results, suspensions, and
//! the first-order continuations suspensions carry. A [`Cont`] is a
//! [`ResumeFrame`] head plus [`Frame`]s, innermost-first, consumed in order.

use std::collections::VecDeque;
use std::sync::Arc;

use pith_core::{
    BodyExpr, BodyRequest, EffectKind, Interface, MatchArm, NominalType, Pure, Request, SumType,
    Value,
};
use pith_diag::DiagnosticSink;

use crate::PureStep;

/// The state of evaluating one rule body segment: finished, waiting on the
/// engine, or failed.
#[derive(Debug)]
pub(super) enum Evaluation {
    /// The segment finished with its value.
    Complete(Value),
    /// The segment needs `step` performed. `then` continues it once the
    /// engine resumes.
    Yield { step: PureStep, then: Cont },
    /// The segment failed with collected diagnostics.
    Failed(DiagnosticSink),
}

/// The continuation of a suspended body: the yield site's handler, plus every
/// frame sequenced behind the suspension.
#[derive(Debug)]
pub(super) struct Cont {
    head: ResumeFrame,
    frames: VecDeque<Frame>,
}

impl Cont {
    /// A continuation that starts at `head` with nothing sequenced behind it.
    #[must_use]
    pub(super) fn awaiting(head: ResumeFrame) -> Self {
        Self {
            head,
            frames: VecDeque::new(),
        }
    }

    /// This continuation with one more frame sequenced behind it.
    #[must_use]
    pub(super) fn pushed(mut self, frame: Frame) -> Self {
        self.frames.push_back(frame);
        self
    }

    /// Move `frames`, already stored innermost-first, behind this
    /// continuation's own.
    pub(super) fn append_frames(&mut self, mut frames: VecDeque<Frame>) {
        self.frames.append(&mut frames);
    }

    /// The head frame and the frames above it, in storage order.
    pub(super) fn into_parts(self) -> (ResumeFrame, VecDeque<Frame>) {
        let Self { head, frames } = self;
        (head, frames)
    }
}

/// The yield site's own continuation: what a suspended body does with the
/// engine's resumption. One variant per yield shape.
#[derive(Debug)]
pub(super) enum ResumeFrame {
    /// The result of one request (`Need`, `NeedBlob`, `NeedAction`,
    /// `NeedObservation`), bound at `Bound(0)`.
    One {
        body: Box<BodyExpr>,
        environment: Environment,
    },
    /// The results of a static batch (`NeedAll`): one binder per result, in
    /// request order.
    PerRequest {
        body: Box<BodyExpr>,
        environment: Environment,
    },
    /// The results of a dynamic batch (`NeedEach`), bound as one list at
    /// `Bound(0)`.
    AsList {
        body: Box<BodyExpr>,
        environment: Environment,
    },
}

/// Which two-operand expression a suspended evaluation sits between the
/// operands of. The operands arrive one suspension at a time.
#[derive(Clone, Copy, Debug)]
pub(super) enum BinaryFinish {
    /// Structural value equality.
    Equal,
    /// Total integer addition.
    IntAdd,
    /// Total integer subtraction.
    IntSubtract,
    /// Total integer multiplication.
    IntMultiply,
    /// Text concatenation.
    TextConcat,
    /// Splitting text on a separator.
    TextBreak,
    /// Joining a list of text with a separator.
    TextJoin,
    /// List concatenation.
    Append,
}

/// The partially evaluated inputs of one request: what a body still owes
/// between the request's first input and the step it can finally yield.
#[derive(Debug)]
pub(super) struct RequestDraft {
    /// The interface the finished request carries.
    pub(super) interface: Interface,
    /// Input expressions not yet evaluated.
    pub(super) pending: std::vec::IntoIter<BodyExpr>,
    /// Values already produced for the inputs evaluated so far.
    pub(super) values: Vec<Value>,
}

/// One sequencing site of the interpreter, waiting for the value of the work
/// suspended beneath it. The variant says what the site was doing. The fields
/// are what it needs to go on.
#[derive(Debug)]
pub(super) enum Frame {
    /// Waiting for `Let`'s bound expression, whose value binds for `rest`.
    Let {
        rest: Box<BodyExpr>,
        environment: Environment,
    },
    /// Waiting for `Fail`'s message, which must be text.
    FailMessage,
    /// Waiting for the record to read `name` from.
    Field { name: Box<str> },
    /// Waiting for the payload of the constructor being built.
    MakeSum {
        declared: SumType,
        constructor: Box<str>,
    },
    /// Waiting for the scrutinee, to select and run one arm.
    Match {
        arms: Box<[MatchArm]>,
        environment: Environment,
    },
    /// Waiting for the representation to wrap in the nominal type.
    Wrap { declared: NominalType },
    /// Waiting for the nominal value to unwrap.
    Unwrap,
    /// Waiting for the value to render as text.
    Describe,
    /// Waiting for the bytes to decode as UTF-8.
    TextOfBytes,
    /// Waiting for the condition that selects `then` or `otherwise`.
    If {
        then: Box<BodyExpr>,
        otherwise: Box<BodyExpr>,
        environment: Environment,
    },
    /// Waiting for the element `Cons` prepends. The tail is evaluated next.
    ConsHead {
        tail: Box<BodyExpr>,
        environment: Environment,
    },
    /// Waiting for the list `Cons` prepends to.
    ConsTail { head: Value },
    /// Waiting for the list to case on, `cons` or `empty`.
    MatchList {
        cons: Box<BodyExpr>,
        empty: Box<BodyExpr>,
        environment: Environment,
    },
    /// Waiting for a two-operand expression's left operand. The right is
    /// evaluated next.
    BinaryLeft {
        finish: BinaryFinish,
        right: BodyExpr,
        environment: Environment,
    },
    /// Waiting for a two-operand expression's right operand, with the left
    /// already in hand.
    BinaryRight { finish: BinaryFinish, left: Value },
    /// Waiting for the fold's source list. The initial accumulator is
    /// evaluated next.
    FoldSource {
        init: Box<BodyExpr>,
        step: Box<BodyExpr>,
        environment: Environment,
    },
    /// Waiting for the accumulator's next value, one element's step in.
    FoldStep {
        pending: std::vec::IntoIter<Value>,
        step: BodyExpr,
        environment: Environment,
    },
    /// Waiting for the list to sort.
    SortBy {
        key: Box<BodyExpr>,
        environment: Environment,
    },
    /// Waiting for one element's sort key.
    SortKey {
        pending: std::vec::IntoIter<Value>,
        key: BodyExpr,
        environment: Environment,
        /// Keys already computed, beside the values they sort.
        keyed: Vec<(Vec<u8>, Value)>,
        /// The element whose key is being computed.
        element: Value,
    },
    /// Waiting for one list literal item.
    ListItems {
        pending: std::vec::IntoIter<BodyExpr>,
        values: Vec<Value>,
        environment: Environment,
    },
    /// Waiting for one record field's payload.
    RecordFields {
        names: std::vec::IntoIter<Box<str>>,
        pending: std::vec::IntoIter<BodyExpr>,
        values: Vec<Value>,
        environment: Environment,
    },
    /// Waiting for one input of the single request being built. `kind` is
    /// the [`EffectKind`] the request targets. The step the draft finally
    /// yields is the kind's `Need` construct.
    RequestInputs {
        kind: EffectKind,
        resume: Box<BodyExpr>,
        environment: Environment,
        draft: RequestDraft,
    },
    /// Waiting for one input of the request a static batch (`NeedAll`) is
    /// building. `pending_requests` are the batch's requests not yet started.
    StaticBatch {
        pending_requests: std::vec::IntoIter<BodyRequest>,
        environment: Environment,
        evaluated: Vec<Request<Pure>>,
        resume: Box<BodyExpr>,
        draft: RequestDraft,
    },
    /// Waiting for one input of the request a dynamic batch (`NeedEach`) is
    /// building for its current element. `template` is instantiated once per
    /// element still in `pending_values`.
    DynamicBatch {
        pending_values: std::vec::IntoIter<Value>,
        template: BodyRequest,
        /// The environment for elements still to come.
        environment: Environment,
        /// The current element's environment, the element at `Bound(0)`.
        element_environment: Environment,
        evaluated: Vec<Request<Pure>>,
        resume: Box<BodyExpr>,
        draft: RequestDraft,
    },
    /// Waiting for `NeedEach`'s source list.
    EachSource {
        template: BodyRequest,
        environment: Environment,
        resume: Box<BodyExpr>,
    },
    /// Waiting for the content identity to fetch.
    BlobContent {
        resume: Box<BodyExpr>,
        environment: Environment,
    },
}

#[derive(Debug)]
struct Binding {
    value: Value,
    previous: Option<Arc<Binding>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Environment {
    latest: Option<Arc<Binding>>,
}

impl Environment {
    pub(super) fn from_inputs(inputs: &[Value]) -> Self {
        inputs.iter().cloned().fold(Self::default(), Self::with)
    }

    pub(super) fn with(self, value: Value) -> Self {
        Self {
            latest: Some(Arc::new(Binding {
                value,
                previous: self.latest,
            })),
        }
    }

    pub(super) fn with_all(self, values: impl IntoIterator<Item = Value>) -> Self {
        values.into_iter().fold(self, Self::with)
    }

    pub(super) fn get(&self, index: usize) -> Option<Value> {
        let mut binding = self.latest.as_deref();
        for _ in 0..index {
            binding = binding?.previous.as_deref();
        }
        binding.map(|binding| binding.value.clone())
    }
}
