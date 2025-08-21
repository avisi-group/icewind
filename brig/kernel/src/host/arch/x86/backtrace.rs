use {
    crate::{host::arch::x86::memory::PhysAddrExt, qemu_exit},
    core::{arch::asm, slice},
    elf::{
        ElfBytes, endian::AnyEndian, parse::ParsingTable, string_table::StringTable, symbol::Symbol,
    },
    spin::Once,
    x86_64::{PhysAddr, VirtAddr},
};

const UNKNOWN: &str = "???";

static BACKTRACER: Once<Backtracer> = Once::INIT;

struct Backtracer {
    kernel_image_virt_addr: VirtAddr,
    kernel_image_len: usize,
    symbol_table: ParsingTable<'static, AnyEndian, Symbol>,
    string_table: StringTable<'static>,
}

impl Backtracer {
    // TODO: explain why the kernel image addresses are different
    fn new(
        // Virtual address of the loaded kernel image
        kernel_image_offset: VirtAddr,
        // Physical address of the kernel ELF in memory
        elf_phys_addr: PhysAddr,
        // Size of the kernel ELF in memory
        elf_len: usize,
    ) -> Self {
        let elf_slice = unsafe { slice::from_raw_parts(elf_phys_addr.to_virt().as_ptr(), elf_len) };
        let elf = ElfBytes::<AnyEndian>::minimal_parse(elf_slice).unwrap();
        let (symbol_table, string_table) = elf.symbol_table().unwrap().unwrap();

        Self {
            kernel_image_virt_addr: kernel_image_offset,
            kernel_image_len: elf_len,
            symbol_table,
            string_table,
        }
    }

    fn lookup_symbol(&self, pc: VirtAddr) -> &'static str {
        // todo: check that it is within kernel range
        // todo: make backtrace struct and this a method on it
        if !(self.kernel_image_virt_addr
            ..self.kernel_image_virt_addr + u64::try_from(self.kernel_image_len).unwrap())
            .contains(&pc)
        {
            return UNKNOWN;
        }

        let pc = pc - self.kernel_image_virt_addr;

        self.symbol_table
            .iter()
            .find(|sym| pc >= sym.st_value && pc < (sym.st_value + sym.st_size))
            .and_then(|sym| {
                self.string_table
                    .get(usize::try_from(sym.st_name).unwrap())
                    .ok()
            })
            .unwrap_or(UNKNOWN)
    }
}

pub fn init(kernel_load_addr: VirtAddr, kernel_image_address: PhysAddr, kernel_image_len: usize) {
    BACKTRACER
        .call_once(|| Backtracer::new(kernel_load_addr, kernel_image_address, kernel_image_len));
}

#[repr(C)]
struct StackFrame {
    rbp: *const StackFrame,
    rip: u64,
}

pub fn backtrace() {
    log::error!("backtrace:");

    let mut stk: *const StackFrame;

    unsafe {
        asm!(
            "mov {0}, rbp",
            out(reg) stk
        )
    }

    while !stk.is_null() {
        let rip = unsafe { &*stk }.rip;
        let pc = match VirtAddr::try_new(rip) {
            Ok(pc) => pc,
            Err(e) => {
                log::error!("    backtrace failed: {e:?}");
                return;
            }
        };

        let symbol = rustc_demangle::demangle(
            BACKTRACER
                .get()
                .map(|bt| bt.lookup_symbol(pc))
                .unwrap_or(UNKNOWN),
        );
        log::error!("    {:x} : {}", pc, symbol);
        unsafe { stk = (*stk).rbp };
    }
}
