#![no_std]
#![feature(allocator_api)]
#![feature(btree_cursors)]

extern crate alloc;

use {
    crate::memory::AddressSpace,
    alloc::boxed::Box,
    core::{alloc::Allocator, fmt::Debug, sync::atomic::AtomicU64},
    x86::current::segmentation::{rdfsbase, wrfsbase},
};

pub use {common::*, proc_macro_lib::*};

pub mod device;
pub mod memory;
pub mod sysreg_helpers;
pub mod tests;

#[repr(C)]
pub struct GuestExecutionContext {
    pub current_address_space: *mut AddressSpace,
    pub interrupt_pending: AtomicU64,
    pub unprivileged_access: u64,
}

impl GuestExecutionContext {
    pub fn activate(self: Box<Self>) {
        unsafe {
            wrfsbase(Box::into_raw(self) as u64);
        }
    }

    pub fn current() -> &'static Self {
        unsafe { &*(rdfsbase() as *const Self) }
    }

    pub fn current_mut() -> &'static mut Self {
        unsafe { &mut *(rdfsbase() as *mut Self) }
    }
}
