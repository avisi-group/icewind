#![no_std]
#![feature(unsafe_cell_access)]
#![feature(allocator_api)]
#![feature(int_roundings)]
#![feature(iter_collect_into)]
#![feature(btreemap_alloc)]

use {
    crate::{register_file::RegisterFile, trampoline::ExecutionResult},
    alloc::{fmt, string::String, vec::Vec},
    core::fmt::Debug,
    iced_x86::{Formatter, Instruction},
    x86_64::{VirtAddr, structures::paging::PageTableFlags},
};

extern crate alloc;

pub mod emitter;
pub mod interpret;
pub mod register_file;
pub mod trampoline;
pub mod translate;
pub mod x86;
