use {
    crate::{
        arch::x86::{memory::guest_physical_to_host_virt, safepoint::interrupt_restore_safepoint},
        guest::{
            GuestExecutionContext, get_current_guest,
            models::{ModelDevice, write_to_el},
        },
    },
    aarch64_paging::descriptor::{Attributes, Descriptor},
    common::sysreg_helpers::encode_sysreg_id,
    core::sync::atomic::{AtomicU64, Ordering},
};

pub const AT_S1E1R: u64 = encode_sysreg_id(0b01, 0b000, 0b0111, 0b1000, 0b000);
pub const DC_ZVA: u64 = encode_sysreg_id(0b01, 0b011, 0b0111, 0b0100, 0b001);
pub const DBGBVR0_EL1: u64 = encode_sysreg_id(2, 0, 0, 0, 4);
pub const DBGBVR1_EL1: u64 = encode_sysreg_id(2, 0, 0, 1, 4);
pub const DBGBCR0_EL1: u64 = encode_sysreg_id(2, 0, 0, 0, 5);
pub const DBGBCR1_EL1: u64 = encode_sysreg_id(2, 0, 0, 1, 5);
pub const DBGWVR0_EL1: u64 = encode_sysreg_id(2, 0, 0, 0, 6);
pub const DBGWVR1_EL1: u64 = encode_sysreg_id(2, 0, 0, 1, 6);
pub const DBGWCR0_EL1: u64 = encode_sysreg_id(2, 0, 0, 0, 7);
pub const DBGWCR1_EL1: u64 = encode_sysreg_id(2, 0, 0, 1, 7);

pub fn at_s1e1r_handler(addr: u64) {
    let device = &get_current_guest().core;

    let _translated_address = guest_translate(device, addr, TranslationType::Translate);
}

pub fn dc_zva_handler(addr: u64) {
    let device = &get_current_guest().core;
    //let _translated_address = guest_translate(device, addr,
    // TranslationType::Translate);
    //panic!("ZVA {addr:#018x}");

    let dczid = device.register_file.read::<u64>("DCZID_EL0_bits");
    unsafe { ((addr & 0xff_ffff_ffff) as *mut u8).write_bytes(0x00, (1 << (dczid & 0xf)) * 4) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExecutionLevel {
    EL0,
    EL1,
    EL2,
    EL3,
}

#[derive(Debug, Clone, Copy)]
struct MmuTranslationContext {
    guest_virtual_address: u64,
    execution_level: ExecutionLevel,
}

static RTLB: [TlbEntry; 4096] = [const { TlbEntry::new() }; 4096];
static WTLB: [TlbEntry; 4096] = [const { TlbEntry::new() }; 4096];
static FTLB: [TlbEntry; 4096] = [const { TlbEntry::new() }; 4096];

struct TlbEntry {
    guest_virt_page: AtomicU64,
    guest_phys_page: AtomicU64,
}

impl TlbEntry {
    pub const fn new() -> Self {
        Self {
            guest_virt_page: AtomicU64::new(u64::MAX),
            guest_phys_page: AtomicU64::new(0),
        }
    }

    pub fn clear(&self) {
        self.guest_virt_page.store(u64::MAX, Ordering::Relaxed);
        self.guest_phys_page.store(0, Ordering::Relaxed);
    }
}

pub fn flush_tlb() {
    RTLB.iter()
        .chain(WTLB.iter())
        .chain(FTLB.iter())
        .for_each(|entry| entry.clear());
}

// returns guest physical address
pub fn guest_translate(
    device: &ModelDevice,
    guest_virtual_address: u64,
    typ: TranslationType,
) -> u64 {
    let mmu_enabled = (device.well_known_registers.sctlr_el1().read() & 1) == 1;
    if !mmu_enabled {
        return guest_virtual_address;
    }

    let tlb = match typ {
        TranslationType::Read => &RTLB,
        TranslationType::Write => &WTLB,
        TranslationType::Fetch => &FTLB,
        TranslationType::Translate => {
            return raw_guest_translate(device, guest_virtual_address, TranslationType::Translate);
        }
    };

    let guest_virtual_page = guest_virtual_address >> 12;
    let guest_virtual_offset = guest_virtual_address & 0xFFF;

    let tlb_entry = &tlb[(guest_virtual_page % 4096) as usize];

    // if entry matches
    if tlb_entry.guest_virt_page.load(Ordering::Relaxed) == guest_virtual_page {
        // use cached value
        tlb_entry.guest_phys_page.load(Ordering::Relaxed) | guest_virtual_offset
    } else {
        // otherwise translate
        let translated_phys_page = raw_guest_translate(device, guest_virtual_page << 12, typ);

        // insert into cache
        tlb_entry
            .guest_phys_page
            .store(translated_phys_page, Ordering::Relaxed);
        tlb_entry
            .guest_virt_page
            .store(guest_virtual_page, Ordering::Relaxed);

        // return translated value
        translated_phys_page | guest_virtual_offset
    }
}

fn raw_guest_translate(
    device: &ModelDevice,
    guest_virtual_address: u64,
    typ: TranslationType,
) -> u64 {
    let ttbr0_el1 = device.register_file.read::<u64>("_TTBR0_EL1_bits");
    let ttbr1_el1 = device.register_file.read::<u64>("_TTBR1_EL1_bits");
    log::trace!("ttbr0_el1: {ttbr0_el1:x}");
    log::trace!("ttbr1_el1: {ttbr1_el1:x}");

    // assumes 39-bit VA
    let top_bit = (guest_virtual_address >> 39) & 1;
    let translation_table_base_guest_phys = match top_bit {
        0 => ttbr0_el1,
        1 => ttbr1_el1,
        _ => unreachable!(),
    };

    let ttbgp_masked = translation_table_base_guest_phys & !0xffff000000000fff;

    log::trace!("guest_virtual_address: {guest_virtual_address:x?}");
    log::trace!("translation_table_base_guest_phys: {translation_table_base_guest_phys:x?}");
    log::trace!("ttbgp_masked: {ttbgp_masked:x?}");

    let translation_table_base = guest_physical_to_host_virt(ttbgp_masked);
    log::trace!("translation_table_base: {translation_table_base:x?}");
    let table = unsafe { &*(translation_table_base.as_ptr::<[Descriptor; 512]>()) };

    //log::trace!("table: {table:x?}");

    // Skip L0, because 3-level page tables.

    let current_el = match device.well_known_registers.pstate_el().read() {
        0 => ExecutionLevel::EL0,
        1 => ExecutionLevel::EL1,
        2 => ExecutionLevel::EL2,
        3 => ExecutionLevel::EL3,
        _ => panic!("not a real el"),
    };

    let effective_execution_level = if GuestExecutionContext::current().unprivileged_access != 0 {
        log::trace!(
            "UNPRIVILEGED ACCESS PC={:x} FA={:x}",
            device.well_known_registers.pc().read(),
            guest_virtual_address
        );

        assert_eq!(current_el, ExecutionLevel::EL1);

        GuestExecutionContext::current_mut().unprivileged_access = 0;
        ExecutionLevel::EL0
    } else {
        current_el
    };

    let mmu_txl_ctx = MmuTranslationContext {
        guest_virtual_address,
        execution_level: effective_execution_level,
    };

    match translate_l1(table, &mmu_txl_ctx, typ) {
        Ok(addr) => {
            if typ == TranslationType::Translate {
                device
                    .register_file
                    .write("_PAR_EL1_bits", addr & 0x0000_ffff_ffff_f000);
            }

            addr
        }
        Err(error) => {
            if typ == TranslationType::Fetch {
                log::trace!(
                    "INSTRUCTION FETCH FAULT PC={:x} FA={:x}",
                    device.well_known_registers.pc().read(),
                    guest_virtual_address
                );
            } else if typ == TranslationType::Translate {
                device
                    .register_file
                    .write::<u64>("_PAR_EL1_bits", (error.to_syndrome() << 1) | 1);

                return 0;
            }

            log::trace!("guest page fault {:x?} {:x?}", mmu_txl_ctx, typ,);

            guest_page_fault(device, &mmu_txl_ctx, error);
            unreachable!();
        }
    }
}

fn translate_l1(
    table: &[Descriptor; 512],
    mmu_txl_ctx: &MmuTranslationContext,
    typ: TranslationType,
) -> Result<u64, TranslationError> {
    let entry_idx = ((mmu_txl_ctx.guest_virtual_address >> 30) & 0x1ff) as usize;
    log::trace!("l1 entry_idx: {entry_idx:x?}");
    let entry = &table[entry_idx];
    log::trace!("l1 entry: {entry:x?}");

    if !entry.is_valid() {
        return Err(TranslationError {
            level: FaultLevel::L1,
            fault_type: FaultType::Translation,
            translation_type: typ,
        });
    }

    if entry.is_table_or_page() {
        return translate_l2(entry_to_table(&entry), mmu_txl_ctx, typ);
    } // else is block

    if !entry.flags().contains(Attributes::ACCESSED) {
        return Err(TranslationError {
            level: FaultLevel::L1,
            fault_type: FaultType::AccessFlag,
            translation_type: typ,
        });
    }

    if !has_permission(mmu_txl_ctx, typ, entry.flags()) {
        return Err(TranslationError {
            level: FaultLevel::L1,
            fault_type: FaultType::Permission,
            translation_type: typ,
        });
    }

    let mask = (1 << 30) - 1;
    Ok((entry.output_address().0 as u64 & !mask) | (mmu_txl_ctx.guest_virtual_address & mask))
}

fn translate_l2(
    table: &[Descriptor; 512],
    mmu_txl_ctx: &MmuTranslationContext,
    typ: TranslationType,
) -> Result<u64, TranslationError> {
    let entry_idx = ((mmu_txl_ctx.guest_virtual_address >> 21) & 0x1ff) as usize;
    log::trace!("l2 entry_idx: {entry_idx:x?}");
    let entry = &table[entry_idx];
    log::trace!("l2 entry: {entry:x?}");

    if !entry.is_valid() {
        return Err(TranslationError {
            level: FaultLevel::L2,
            fault_type: FaultType::Translation,
            translation_type: typ,
        });
    }

    if entry.is_table_or_page() {
        return translate_l3(entry_to_table(&entry), mmu_txl_ctx, typ);
    } // else is block

    if !entry.flags().contains(Attributes::ACCESSED) {
        return Err(TranslationError {
            level: FaultLevel::L2,
            fault_type: FaultType::AccessFlag,
            translation_type: typ,
        });
    }

    if !has_permission(mmu_txl_ctx, typ, entry.flags()) {
        return Err(TranslationError {
            level: FaultLevel::L2,
            fault_type: FaultType::Permission,
            translation_type: typ,
        });
    }

    let mask = (1 << 21) - 1;
    Ok((entry.output_address().0 as u64 & !mask) | (mmu_txl_ctx.guest_virtual_address & mask))
}

fn translate_l3(
    table: &[Descriptor; 512],
    mmu_txl_ctx: &MmuTranslationContext,
    typ: TranslationType,
) -> Result<u64, TranslationError> {
    let entry_idx = ((mmu_txl_ctx.guest_virtual_address >> 12) & 0x1ff) as usize;
    log::trace!("l3 entry_idx: {entry_idx:x?}");
    let entry = &table[entry_idx];
    log::trace!("l3 entry: {entry:x?}");

    if !entry.is_table_or_page() {
        return Err(TranslationError {
            level: FaultLevel::L3,
            fault_type: FaultType::Translation,
            translation_type: typ,
        });
    }

    if !entry.flags().contains(Attributes::ACCESSED) {
        return Err(TranslationError {
            level: FaultLevel::L3,
            fault_type: FaultType::AccessFlag,
            translation_type: typ,
        });
    }

    if !has_permission(mmu_txl_ctx, typ, entry.flags()) {
        return Err(TranslationError {
            level: FaultLevel::L3,
            fault_type: FaultType::Permission,
            translation_type: typ,
        });
    }

    Ok((entry.output_address().0 as u64) | (mmu_txl_ctx.guest_virtual_address & ((1 << 12) - 1)))
}

fn entry_to_table(entry: &Descriptor) -> &[Descriptor; 512] {
    unsafe {
        &*guest_physical_to_host_virt(entry.output_address().0 as u64).as_ptr::<[Descriptor; 512]>()
    }
}

fn guest_page_fault(
    device: &ModelDevice,
    mmu_txl_ctx: &MmuTranslationContext,
    error: TranslationError,
) {
    log::warn!(
        "guest page fault @ {:0x}",
        mmu_txl_ctx.guest_virtual_address
    );

    let retaddr = device.well_known_registers.pc().read();

    let typ = if error.translation_type == TranslationType::Fetch {
        4
    } else {
        1
    };

    let syndrome = error.to_syndrome();

    // TODO: Consider behaviour for STTR
    take_arm_exception(
        device,
        1,
        typ,
        syndrome,
        mmu_txl_ctx.guest_virtual_address,
        retaddr,
        0,
    );

    interrupt_restore_safepoint(1);
}

pub fn take_arm_exception(
    device: &ModelDevice,
    target_el: u8,
    typ: u8,
    syndrome: u64,
    vaddr: u64,
    retaddr: u64,
    mut voff: u64,
) {
    log::trace!(
        "called take_arm_exception: target_el={target_el:#x}, typ={typ:#x}, syndrom={syndrome:#x}, retaddr={retaddr:#x}"
    );
    let spsr = get_psr_from_pstate(device);
    log::trace!("spsr: {spsr:032b}");

    let current_el = device.well_known_registers.pstate_el().read();
    log::trace!("current_el: {current_el}");

    if target_el > current_el {
        voff += 0x400;
    } else if device.register_file.read::<u8>("PSTATE_SP") == 1 {
        voff += 0x200;
    }

    log::trace!("voff: {voff:x}");

    // Update the execution level
    write_to_el(current_el, target_el);
    device.register_file.write::<u8>("PSTATE_EL", target_el);

    // Update spsel
    device.register_file.write::<u8>("PSTATE_SP", 1);

    if target_el == 1 {
        device.register_file.write::<u32>("SPSR_EL1_bits", spsr);
        device.register_file.write::<u64>("ELR_EL1", retaddr);

        // If it's NOT an IRQ...
        if typ != 255 {
            let ec = get_exception_class(current_el, target_el, typ);
            device.register_file.write::<u64>(
                "ESR_EL1_bits",
                (ec << 26) | (1 << 25) | (syndrome & 0x1ffffff),
            );

            if typ == 1 || typ == 4 {
                device.register_file.write::<u64>("FAR_EL1", vaddr);
            }
        }
    } else {
        panic!("trap");
    }

    device.register_file.write::<u8>("PSTATE_D", 1);
    device.register_file.write::<u8>("PSTATE_A", 1);
    device.register_file.write::<u8>("PSTATE_I", 1);
    device.register_file.write::<u8>("PSTATE_F", 1);

    let vbar = device.register_file.read::<u64>(match target_el {
        1 => "VBAR_EL1",
        2 => "VBAR_EL2",
        3 => "VBAR_EL3",
        _ => panic!("invalid EL \"{target_el}\""),
    });
    log::trace!("vbar: {:x}", vbar);

    log::trace!("pc: {:x}", vbar + voff);
    device.register_file.write::<u64>("_PC", vbar + voff);
}

fn get_psr_from_pstate(device: &ModelDevice) -> u32 {
    let n = device.register_file.read::<u8>("PSTATE_N") as u32;
    let z = device.register_file.read::<u8>("PSTATE_Z") as u32;
    let c = device.register_file.read::<u8>("PSTATE_C") as u32;
    let v = device.register_file.read::<u8>("PSTATE_V") as u32;
    let d = device.register_file.read::<u8>("PSTATE_D") as u32;
    let a = device.register_file.read::<u8>("PSTATE_A") as u32;
    let i = device.register_file.read::<u8>("PSTATE_I") as u32;
    let f = device.register_file.read::<u8>("PSTATE_F") as u32;
    let el = device.register_file.read::<u8>("PSTATE_EL") as u32;
    let sp = device.register_file.read::<u8>("PSTATE_SP") as u32;

    n << 31 | z << 30 | c << 29 | v << 28 | d << 9 | a << 8 | i << 7 | f << 6 | el << 2 | sp
}

fn get_exception_class(current_el: u8, target_el: u8, typ: u8) -> u64 {
    match typ {
        0 => {
            // Software Breakpoint
            0x38 + 4
        }
        1 => {
            // Data Fault
            if target_el == current_el { 0x25 } else { 0x24 }
        }

        2 => {
            // Undefined Fault
            0
        }
        3 => {
            // Supervisor Call
            0x11 + 4
        }
        4 => {
            // Instruction Abort
            if target_el == current_el { 0x21 } else { 0x20 }
        }
        5 => {
            // FPAccessTrap
            0x07
        }
        6 => {
            // Single Step
            0x32
        }
        _ => {
            panic!("trap")
        }
    }
}

struct TranslationError {
    level: FaultLevel,
    fault_type: FaultType,
    translation_type: TranslationType,
}

impl TranslationError {
    pub fn to_syndrome(&self) -> u64 {
        let level = match self.level {
            FaultLevel::L0 => 0,
            FaultLevel::L1 => 1,
            FaultLevel::L2 => 2,
            FaultLevel::L3 => 3,
        };

        let typ = match self.fault_type {
            FaultType::AddressSize => 0,
            FaultType::Translation => 4,
            FaultType::AccessFlag => 8,
            FaultType::Permission => 12,
        };

        //    ctx.private_data = (uint64_t) type | (uint64_t) level;

        // if (ctx.type == AddressTranslationType::WRITE) {
        // 	ctx.private_data |= 0x40;
        // }

        typ | level
            | if self.translation_type == TranslationType::Write {
                0x40
            } else if self.translation_type == TranslationType::Translate {
                0x100
            } else {
                0x0
            }
    }
}

enum FaultLevel {
    L0,
    L1,
    L2,
    L3,
}

enum FaultType {
    AddressSize,
    Translation,
    AccessFlag,
    Permission,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranslationType {
    Read,
    Write,
    Fetch,
    Translate,
}

fn has_permission(
    mmu_txl_ctx: &MmuTranslationContext,
    typ: TranslationType,
    entry_flags: Attributes,
) -> bool {
    let user = entry_flags.contains(Attributes::USER);
    let read_only = entry_flags.contains(Attributes::READ_ONLY);

    match (user, read_only) {
        // EL1 RW, EL0 -
        (false, false) => {
            if mmu_txl_ctx.execution_level == ExecutionLevel::EL0 {
                false
            } else {
                true
            }
        }

        // EL1 RW, EL0 RW
        (true, false) => true,

        // EL1 RO, EL0 -
        (false, true) => {
            if mmu_txl_ctx.execution_level == ExecutionLevel::EL0 {
                false
            } else {
                typ == TranslationType::Read
                    || typ == TranslationType::Translate
                    || typ == TranslationType::Fetch
            }
        }

        // EL1 RO, EL0 RO
        (true, true) => {
            typ == TranslationType::Read
                || typ == TranslationType::Translate
                || typ == TranslationType::Fetch
        }
    }
}
