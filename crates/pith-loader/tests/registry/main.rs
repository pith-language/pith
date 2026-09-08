//! The registry index and its client: publication derives and signs, the
//! checks refuse each at their own boundary, and the same module bytes
//! loaded locally and through the signed index elaborate identically.
//! Fixed fixture seeds keep an on-disk index a function of its publishes.

#[path = "../common/mod.rs"]
mod common;

mod acquisition;
mod fixture;
mod refusals;
mod round_trip;
