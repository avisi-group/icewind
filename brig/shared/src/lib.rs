#![no_std]
#![feature(allocator_api)]

use core::{alloc::Allocator, fmt::Debug};

/// Allocator convenience trait
pub trait Alloc: Allocator + Clone + Copy + Debug {}

// implement Alloc on everything that implements it's constituent traits
impl<T: Allocator + Clone + Copy + Debug> Alloc for T {}
