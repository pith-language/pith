//! Brand-typed arena indices and interners.
//!
//! Each arena category gets a distinct brand and therefore a distinct `Id`
//! type, so ids from different categories do not type-check against each other
//! (decision 0005). `la_arena` is the backing store, wrapped here so it never
//! appears in public types outside this module.

use std::marker::PhantomData;

/// Marker trait for brand types, one per arena category.
pub trait Brand:
    'static + Clone + Copy + Send + Sync + std::fmt::Debug + Eq + std::hash::Hash
{
}

/// A branded index into an arena. `B` identifies the arena category; ids from
/// different categories are different types and do not interoperate.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id<B: Brand> {
    raw: u32,
    _brand: PhantomData<fn() -> B>,
}

impl<B: Brand> Id<B> {
    pub fn from_raw(raw: u32) -> Self {
        Self {
            raw,
            _brand: PhantomData,
        }
    }

    pub fn to_raw(self) -> u32 {
        self.raw
    }
}

/// An arena storing `T` and returning `Id<B>` on insert. The only way to mint
/// a valid `Id<B>` for a stored value is [`Arena::push`] (or `Interner::intern`).
pub struct Arena<B: Brand, T> {
    inner: la_arena::Arena<T>,
    _brand: PhantomData<fn() -> B>,
}

impl<B: Brand, T> Arena<B, T> {
    pub fn new() -> Self {
        Self {
            inner: la_arena::Arena::new(),
            _brand: PhantomData,
        }
    }

    pub fn push(&mut self, value: T) -> Id<B> {
        let idx = self.inner.alloc(value);
        Id::from_raw(idx.into_raw().into())
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn get(&self, id: Id<B>) -> Option<&T> {
        let raw: u32 = id.raw;
        if (raw as usize) >= self.inner.len() {
            return None;
        }
        let idx = la_arena::Idx::from_raw(la_arena::RawIdx::from(raw));
        #[allow(clippy::indexing_slicing)]
        Some(&self.inner[idx])
    }

    pub fn iter(&self) -> impl Iterator<Item = (Id<B>, &T)> {
        self.inner
            .iter()
            .map(|(idx, v)| (Id::from_raw(idx.into_raw().into()), v))
    }
}

impl<B: Brand, T> Default for Arena<B, T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Arena that deduplicates equal values to a shared `Id<B>`.
pub struct Interner<B: Brand, T: std::hash::Hash + Eq> {
    arena: Arena<B, T>,
    index: indexmap::IndexMap<T, Id<B>>,
}

impl<B: Brand, T: std::hash::Hash + Eq + Clone> Interner<B, T> {
    pub fn new() -> Self {
        Self {
            arena: Arena::new(),
            index: indexmap::IndexMap::new(),
        }
    }

    pub fn intern(&mut self, value: T) -> Id<B> {
        if let Some(id) = self.index.get(&value) {
            return *id;
        }
        let id = self.arena.push(value.clone());
        self.index.insert(value, id);
        id
    }

    pub fn get(&self, id: Id<B>) -> Option<&T> {
        self.arena.get(id)
    }
}

impl<B: Brand, T: std::hash::Hash + Eq + Clone> Default for Interner<B, T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Define an arena category: a brand struct and `$id` / `$arena` aliases.
/// Distinct invocations produce incompatible id types.
#[macro_export]
macro_rules! define_arena {
    ($id:ident, $arena:ident, $brand:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $brand;
        impl $crate::Brand for $brand {}
        pub type $id = $crate::Id<$brand>;
        pub type $arena<T> = $crate::Arena<$brand, T>;
    };
    ($id:ident, $arena:ident, $brand:ident, $doc:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $brand;
        impl $crate::Brand for $brand {}
        #[doc = $doc]
        pub type $id = $crate::Id<$brand>;
        pub type $arena<T> = $crate::Arena<$brand, T>;
    };
}

#[cfg(test)]
mod tests {
    #![allow(dead_code)]
    use super::*;

    define_arena!(TestId, TestArena, TestBrand);
    define_arena!(OtherTestId, OtherTestArena, OtherTestBrand);

    #[test]
    fn push_then_get_round_trips() {
        let mut arena: TestArena<u32> = Arena::new();
        let id = arena.push(10u32);
        assert_eq!(arena.get(id), Some(&10));
    }

    #[test]
    fn out_of_range_get_is_none() {
        let arena: TestArena<u32> = Arena::new();
        assert_eq!(arena.get(TestId::from_raw(99)), None);
    }

    #[test]
    fn ids_of_distinct_categories_are_distinct_types() {
        fn name<T>() -> &'static str {
            std::any::type_name::<T>()
        }
        assert_ne!(name::<TestId>(), name::<OtherTestId>());
    }

    #[test]
    fn interner_deduplicates_equal_values() {
        let mut ix: Interner<TestBrand, Box<str>> = Interner::new();
        let a = ix.intern("x".into());
        let b = ix.intern("x".into());
        let c = ix.intern("y".into());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
