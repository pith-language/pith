//! The registry index and its client: publication derives and signs, the
//! four checks refuse each at their own boundary, and the same module bytes
//! loaded locally and through the signed index elaborate identically.
//!
//! Every fixture runs one directory host with fixed seeds and fixed
//! admission times, so an index on disk is a function of what was
//! published to it and nothing else.

#[path = "../common/mod.rs"]
mod common;

mod acquisition;
mod fixture;
mod refusals;
mod round_trip;
