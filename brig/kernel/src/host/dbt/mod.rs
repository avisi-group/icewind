use {
    crate::host::{arch::x86::memory::VirtualMemoryArea, dbt::trampoline::ExecutionResult},
    alloc::{string::String, vec::Vec},
    core::fmt::{self, Debug},
    dbt::register_file::RegisterFile,
    iced_x86::{Formatter, Instruction},
    x86_64::{VirtAddr, structures::paging::PageTableFlags},
};

pub mod dag;
pub mod emitter;
pub mod sysreg_helpers;
mod trampoline;
pub mod translate;
pub mod x86;

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
