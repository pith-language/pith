//! Brand-typed arena indices and interners.
//!
//! Category brands prevent type substitution, and private owner tokens reject
//! ids from another arena instance (decisions 0005, 0021).

use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ARENA_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct ArenaOwner(u64);

fn fresh_owner() -> ArenaOwner {
    match NEXT_ARENA_OWNER.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        current.checked_add(1)
    }) {
        Ok(owner) => ArenaOwner(owner),
        Err(_) => std::process::abort(),
    }
}

/// Marker trait for brand types, one per arena category.
pub trait Brand:
    'static + Clone + Copy + Send + Sync + std::fmt::Debug + Eq + std::hash::Hash
{
}

/// A branded index into an arena. `B` identifies the arena category; ids from
/// different categories are different types and do not interoperate. IDs have
/// no public constructor.
///
/// ```compile_fail
/// use pith_arena::{Brand, Id};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// struct ExampleBrand;
/// impl Brand for ExampleBrand {}
///
/// let _ = Id::<ExampleBrand>::from_raw(0);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Id<B: Brand> {
    owner: ArenaOwner,
    raw: u32,
    _brand: PhantomData<fn() -> B>,
}

impl<B: Brand> Id<B> {
    fn new(owner: ArenaOwner, raw: u32) -> Self {
        Self {
            owner,
            raw,
            _brand: PhantomData,
        }
    }
}

impl<B: Brand> std::fmt::Debug for Id<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Id").field(&self.raw).finish()
    }
}

/// An arena storing `T` and returning `Id<B>` on insert. The only way to mint
/// a valid `Id<B>` for a stored value is [`Arena::push`] (or `Interner::intern`).
pub struct Arena<B: Brand, T> {
    owner: ArenaOwner,
    inner: la_arena::Arena<T>,
    _brand: PhantomData<fn() -> B>,
}

impl<B: Brand, T> Arena<B, T> {
    pub fn new() -> Self {
        Self {
            owner: fresh_owner(),
            inner: la_arena::Arena::new(),
            _brand: PhantomData,
        }
    }

    pub fn push(&mut self, value: T) -> Id<B> {
        let idx = self.inner.alloc(value);
        Id::new(self.owner, idx.into_raw().into())
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn get(&self, id: Id<B>) -> Option<&T> {
        if id.owner != self.owner {
            return None;
        }
        let raw: u32 = id.raw;
        if (raw as usize) >= self.inner.len() {
            return None;
        }
        let idx = la_arena::Idx::from_raw(la_arena::RawIdx::from(raw));
        #[allow(
            clippy::indexing_slicing,
            reason = "the explicit length check above makes this arena index valid"
        )]
        Some(&self.inner[idx])
    }

    pub fn get_mut(&mut self, id: Id<B>) -> Option<&mut T> {
        if id.owner != self.owner {
            return None;
        }
        let raw: u32 = id.raw;
        if (raw as usize) >= self.inner.len() {
            return None;
        }
        let idx = la_arena::Idx::from_raw(la_arena::RawIdx::from(raw));
        #[allow(
            clippy::indexing_slicing,
            reason = "the explicit length check above makes this arena index valid"
        )]
        Some(&mut self.inner[idx])
    }

    pub fn iter(&self) -> impl Iterator<Item = (Id<B>, &T)> {
        let owner = self.owner;
        self.inner
            .iter()
            .map(move |(idx, v)| (Id::new(owner, idx.into_raw().into()), v))
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
    #![allow(
        dead_code,
        reason = "the second generated arena alias exists only to prove brand separation"
    )]
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
        assert_eq!(arena.get(TestId::new(arena.owner, 99)), None);
    }

    #[test]
    fn id_from_another_arena_is_rejected() {
        let mut first: TestArena<u32> = Arena::new();
        let mut second: TestArena<u32> = Arena::new();
        let first_id = first.push(10);
        let second_id = second.push(20);

        assert_ne!(first_id, second_id);
        assert_eq!(first.get(second_id), None);
        assert_eq!(second.get(first_id), None);
        assert_eq!(second.get_mut(first_id), None);
    }

    #[test]
    fn debug_output_does_not_expose_the_owner_token() {
        let mut first: TestArena<u32> = Arena::new();
        let mut second: TestArena<u32> = Arena::new();
        let first_id = first.push(10);
        let second_id = second.push(20);

        assert_eq!(format!("{first_id:?}"), "Id(0)");
        assert_eq!(format!("{first_id:?}"), format!("{second_id:?}"));
    }

    #[test]
    fn get_mut_updates_a_stored_value() {
        let mut arena: TestArena<u32> = Arena::new();
        let id = arena.push(10u32);
        if let Some(value) = arena.get_mut(id) {
            *value = 11;
        }
        assert_eq!(arena.get(id), Some(&11));
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
