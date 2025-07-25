use {
    crate::host::{
        arch::x86::memory::VirtualMemoryArea,
        dbt::{register_file::RegisterFile, trampoline::ExecutionResult},
    },
    alloc::{string::String, vec::Vec},
    common::mask::mask,
    core::{
        alloc::Allocator,
        fmt::{self, Debug},
    },
    iced_x86::{Formatter, Instruction},
    x86_64::{VirtAddr, structures::paging::PageTableFlags},
};

pub mod emitter;
pub mod interpret;
pub mod models;
pub mod register_file;
pub mod sysreg_helpers;
mod tests;
mod trampoline;
pub mod translate;
pub mod x86;

/// Allocator convenience trait
pub trait Alloc: Allocator + Clone + Copy + Debug {}

// implement Alloc on everything that implements it's constituent traits
impl<T: Allocator + Clone + Copy + Debug> Alloc for T {}

pub struct Translation {
    // should be AlignedAllocator<4096> or ExecutableAllocator
    pub code: Vec<u8>,
}

impl Translation {
    pub fn new(code: Vec<u8>) -> Self {
        let start = VirtAddr::from_ptr(code.as_ptr());
        VirtualMemoryArea::current().update_flags_range(
            start..start + code.len() as u64,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE, // removing  "NOEXECUTE" flag
        );
        Self { code }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.code.as_ptr()
    }

    pub fn execute(&self, register_file: &RegisterFile) -> ExecutionResult {
        let code_ptr = self.as_ptr();
        let register_file_ptr = register_file.as_mut_ptr();

        trampoline::trampoline(code_ptr, register_file_ptr)
    }
}

/// Disabled until we can validate that `code` is always page-aligned: after the
/// variable deep clone fix we got isntruction fetch host page faults when
/// executing cached translations, likely because another translation drop
/// overlapped?
// impl Drop for Translation {
//     fn drop(&mut self) {
//         let start = VirtAddr::from_ptr(self.code.as_ptr());
//         VirtualMemoryArea::current().update_flags_range(
//             start..start + self.code.len() as u64,
//             PageTableFlags::PRESENT | PageTableFlags::WRITABLE |
// PageTableFlags::NO_EXECUTE,         );
//     }
// }

impl Debug for Translation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut decoder = iced_x86::Decoder::with_ip(64, &self.code, 0, 0);

        let mut formatter = iced_x86::GasFormatter::new();

        let mut output = String::new();

        let mut instr = Instruction::default();

        while decoder.can_decode() {
            output.clear();
            decoder.decode_out(&mut instr);
            formatter.format(&instr, &mut output);
            writeln!(f, "{:016x} {output}", instr.ip())?;
        }

        Ok(())
    }
}

fn bit_insert(target: u128, source: u128, start: u32, length: u32) -> u128 {
    // todo: hack
    if start >= 64 {
        if source == 0 {
            return 0;
        } else {
            panic!("attempting to insert {length} bits of {source} into {target} at {start}");
        }
    }

    let cleared_target = {
        let mask = !(mask(length).checked_shl(start).unwrap_or_else(|| {
            panic!("overflow in shl with {target:b} {source:?} {start:?} {length:?}")
        }));
        target & mask
    };

    let shifted_source = {
        let mask = mask(length);
        let masked_source = source & mask;
        masked_source << start
    };

    cleared_target | shifted_source
}

fn bit_extract(value: u128, start: u32, length: u32) -> u128 {
    (value >> start) & mask(length)
}
