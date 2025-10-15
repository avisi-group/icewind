#![no_std]
#![feature(allocator_api)]

extern crate alloc;

pub use hashbrown::hash_map::Entry;
use {
    alloc::{string::String, vec::Vec},
    serde::{Deserialize, Serialize},
};

pub mod arena;
pub mod fuzz_test;
pub mod hashmap;
pub mod id;
pub mod intern;
pub mod mask;
pub mod ringbuffer;
pub mod rudder;
pub mod width_helpers;

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
