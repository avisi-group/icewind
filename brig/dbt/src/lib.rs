#![no_std]
#![feature(unsafe_cell_access)]
#![feature(allocator_api)]
#![feature(int_roundings)]
#![feature(iter_collect_into)]
#![feature(btreemap_alloc)]

extern crate alloc;

pub mod bump_alloc;
pub mod emitter;
pub mod interpret;
pub mod register_file;
pub mod trampoline;
pub mod translate;
pub mod x86;
