use crate::{Arena, Interner};

define_arena!(SampleId, SampleArena, SampleBrand);

#[test]
fn new_arena_is_empty_with_zero_length() {
    let arena: SampleArena<u32> = Arena::new();
    assert!(arena.is_empty());
    assert_eq!(arena.len(), 0);
}

#[test]
fn default_equals_new() {
    let arena: SampleArena<u32> = Arena::default();
    assert!(arena.is_empty());
}

#[test]
fn push_increments_length() {
    let mut arena: SampleArena<u32> = Arena::new();
    let _ = arena.push(1);
    let _ = arena.push(2);
    let _ = arena.push(3);
    assert_eq!(arena.len(), 3);
    assert!(!arena.is_empty());
}

#[test]
fn iter_yields_ids_in_insertion_order_with_values() {
    let mut arena: SampleArena<u32> = Arena::new();
    let a = arena.push(1);
    let b = arena.push(2);
    let c = arena.push(3);

    let collected: Vec<(SampleId, u32)> = arena.iter().map(|(id, v)| (id, *v)).collect();
    assert_eq!(collected, [(a, 1), (b, 2), (c, 3)]);
}

#[test]
fn get_mut_returns_none_for_an_id_from_another_arena() {
    let mut first: SampleArena<u32> = Arena::new();
    let mut second: SampleArena<u32> = Arena::new();
    let first_id = first.push(10);
    let second_id = second.push(20);

    assert_eq!(first.get(second_id), None);
    assert_eq!(second.get(first_id), None);
    assert_eq!(first.get_mut(second_id), None);
    assert_eq!(second.get_mut(first_id), None);
}

#[test]
fn ids_from_distinct_arenas_are_not_equal_even_for_equal_raw_indices() {
    let mut first: SampleArena<u32> = Arena::new();
    let mut second: SampleArena<u32> = Arena::new();
    let first_id = first.push(1);
    let second_id = second.push(2);
    assert_ne!(first_id, second_id);
}

#[test]
fn interner_get_returns_the_interned_value() {
    let mut ix: Interner<SampleBrand, Box<str>> = Interner::new();
    let id = ix.intern("hello".into());
    assert_eq!(ix.get(id), Some(&"hello".into()));
}

#[test]
fn interner_distinct_values_get_distinct_ids() {
    let mut ix: Interner<SampleBrand, u32> = Interner::new();
    let a = ix.intern(1);
    let b = ix.intern(2);
    let again = ix.intern(1);
    assert_ne!(a, b);
    assert_eq!(a, again);
    assert_eq!(ix.get(a), Some(&1));
    assert_eq!(ix.get(b), Some(&2));
}

#[test]
fn interner_does_not_deduplicate_unequal_values_that_compare_equal_as_bytes() {
    // Interner keys on `Hash + Eq`; distinct values stay distinct.
    let mut ix: Interner<SampleBrand, (u32, u32)> = Interner::new();
    let a = ix.intern((1, 2));
    let b = ix.intern((1, 3));
    assert_ne!(a, b);
}

#[test]
fn id_debug_format_is_stable_and_owner_free() {
    let mut arena: SampleArena<u32> = Arena::new();
    let id = arena.push(0);
    assert_eq!(format!("{id:?}"), "Id(0)");
}
