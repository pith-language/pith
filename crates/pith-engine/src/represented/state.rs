use std::sync::Arc;

use pith_core::Value;
use pith_diag::DiagnosticSink;

use crate::{PureStep, Resumption};

pub(super) type Resume<T> = Box<dyn FnOnce(Resumption) -> Evaluation<T> + Send>;

pub(super) enum Evaluation<T> {
    Complete(T),
    Yield { step: PureStep, resume: Resume<T> },
    Failed(DiagnosticSink),
}

impl<T: Send + 'static> Evaluation<T> {
    pub(super) fn and_then<U: Send + 'static>(
        self,
        continuation: impl FnOnce(T) -> Evaluation<U> + Send + 'static,
    ) -> Evaluation<U> {
        match self {
            Self::Complete(value) => continuation(value),
            Self::Yield { step, resume } => Evaluation::Yield {
                step,
                resume: Box::new(move |resumption| resume(resumption).and_then(continuation)),
            },
            Self::Failed(diagnostics) => Evaluation::Failed(diagnostics),
        }
    }
}

struct Binding {
    value: Value,
    previous: Option<Arc<Binding>>,
}

#[derive(Clone, Default)]
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
