//! A selected dependency closure, in the order elaboration walks it.
//!
//! The root is a field, not the last element of a list, so asking for it is
//! total: a resolution that produced no root produced no closure.

/// Every module of a selected closure: the dependencies, each preceding its
/// own consumers, and the root the closure was selected for.
pub struct Closure<M> {
    dependencies: Box<[M]>,
    root: M,
}

impl<M> Closure<M> {
    /// Split a dependency-ordered list into the dependencies and the root
    /// that terminates it, or `None` when the list is empty.
    pub(super) fn from_dependency_order(mut modules: Vec<M>) -> Option<Self> {
        let root = modules.pop()?;
        Some(Self {
            dependencies: modules.into(),
            root,
        })
    }

    /// The module the closure was selected for.
    #[must_use]
    pub const fn root(&self) -> &M {
        &self.root
    }

    /// The root's transitive dependencies, each preceding its consumers.
    #[must_use]
    pub fn dependencies(&self) -> &[M] {
        &self.dependencies
    }

    /// Every module in elaboration order: dependencies first, root last.
    pub fn modules(&self) -> impl Iterator<Item = &M> {
        self.dependencies.iter().chain(std::iter::once(&self.root))
    }

    /// How many modules the closure holds, never zero.
    #[must_use]
    pub fn len(&self) -> std::num::NonZeroUsize {
        std::num::NonZeroUsize::MIN.saturating_add(self.dependencies.len())
    }

    /// Consume into the root and its dependencies.
    #[must_use]
    pub fn into_parts(self) -> (M, Box<[M]>) {
        (self.root, self.dependencies)
    }

    /// Rebuild the closure over another module representation, preserving
    /// the order and the root's position.
    pub(super) fn try_map<N, E>(
        self,
        mut project: impl FnMut(M) -> Result<N, E>,
    ) -> Result<Closure<N>, E> {
        let dependencies = self
            .dependencies
            .into_vec()
            .into_iter()
            .map(&mut project)
            .collect::<Result<Vec<_>, E>>()?;
        Ok(Closure {
            dependencies: dependencies.into(),
            root: project(self.root)?,
        })
    }

    /// Walk the closure in dependency order, projecting each module against
    /// everything already projected. The root is projected last, so it sees
    /// its whole closure; nothing else sees it.
    pub(super) fn try_scan<'closure, N, E>(
        &'closure self,
        mut project: impl FnMut(&'closure M, &[(&'closure M, N)]) -> Result<N, E>,
    ) -> Result<Closure<(&'closure M, N)>, E> {
        let mut dependencies = Vec::with_capacity(self.dependencies.len());
        for module in &self.dependencies {
            let projected = project(module, &dependencies)?;
            dependencies.push((module, projected));
        }
        let root = project(&self.root, &dependencies)?;
        Ok(Closure {
            dependencies: dependencies.into(),
            root: (&self.root, root),
        })
    }
}
