#![no_std]
#![feature(allocator_api)]

extern crate alloc;

pub use hashbrown::hash_map::Entry;

use {
    alloc::{string::String, vec::Vec},
    byte_unit::{AdjustedByte, Byte, UnitType},
    serde::{Deserialize, Serialize},
};

pub mod arena;
pub mod bits;
pub mod fuzz_test;
pub mod hashmap;
pub mod id;
pub mod intern;
pub mod ringbuffer;
pub mod rudder;
pub mod width_helpers;

pub enum TracingMode {
    // No tracing
    None,

    // Calls to functions inserted a beginning of blocks writing current PC to shared memory
    // ringbuffer
    Software,

    // PTWRITE instruction used
    PtWrite,
}

pub const TRACING_MODE: TracingMode = TracingMode::PtWrite;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TestConfig {
    // Do not run tests
    None,
    // Only run the specified tests
    Include(Vec<String>),
    // Run all tests except those specified
    Exclude(Vec<String>),
    // Run all tests
    All,
}

pub fn bytes<T>(n: T) -> AdjustedByte
where
    Byte: From<T>,
{
    Byte::from(n).get_appropriate_unit(UnitType::Binary)
}
