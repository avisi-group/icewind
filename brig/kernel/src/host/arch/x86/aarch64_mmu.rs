use {
    crate::{
        host::{
            arch::x86::{
                irq::exit_with_message, memory::guest_physical_to_host_virt,
                safepoint::interrupt_restore_safepoint,
            },
            dbt::models::ModelDevice,
        },
        qemu_exit,
    },
    aarch64_paging::paging::Descriptor,
    proc_macro_lib::ktest,
};

// returns guest physical address
pub fn guest_translate(device: &ModelDevice, guest_virtual_address: u64) -> Option<u64> {
    let mmu_enabled = device.register_file.read::<u64>("SCTLR_EL1_bits") & 1 == 1;
    if !mmu_enabled {
        return Some(guest_virtual_address);
    }
    // let ttbcr = device.register_file.read::<u32>("TTBCR_S_bits");
    // log::trace!("{ttbcr:032b}");
    // let ttbcr_n = ttbcr & 0b111;
    // exit_with_message!("{ttbcr_n:03b}");

    let ttbr0_el1 = device.register_file.read::<u64>("_TTBR0_EL1_bits");
    let ttbr1_el1 = device.register_file.read::<u64>("_TTBR1_EL1_bits");
    log::trace!("ttbr0_el1: {ttbr0_el1:x}");
    log::trace!("ttbr1_el1: {ttbr1_el1:x}");

    // assumes 39-bit VA
    let top_bit = (guest_virtual_address >> 39) & 1;
    let translation_table_base_guest_phys = match top_bit {
        0 => ttbr0_el1,
        1 => ttbr1_el1,
        _ => exit_with_message!(""),
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
    translate_l1(device, table, guest_virtual_address)
}

fn _translate_l0(
    device: &ModelDevice,
    table: &[Descriptor; 512],
    guest_virtual_address: u64,
) -> Option<u64> {
    let entry_idx = ((guest_virtual_address >> 39) & 0x1ff) as usize;

    log::trace!("entry_idx: {entry_idx:x?}");
    let entry = &table[entry_idx];
    log::trace!("entry: {entry:x?}");

    if entry.is_table_or_page() {
        translate_l1(device, entry_to_table(&entry), guest_virtual_address)
    } else {
        exit_with_message!("entry was not table or page")
    }
}

fn translate_l1(
    device: &ModelDevice,
    table: &[Descriptor; 512],
    guest_virtual_address: u64,
) -> Option<u64> {
    let entry_idx = ((guest_virtual_address >> 30) & 0x1ff) as usize;
    log::trace!("l1 entry_idx: {entry_idx:x?}");
    let entry = &table[entry_idx];
    log::trace!("l1 entry: {entry:x?}");

    if !entry.is_valid() {
        // guest page fault, look up exception vector table (VBAR_EL2)
        guest_page_fault(device, guest_virtual_address);
        exit_with_message!("invalid L1")
    }

    if entry.is_table_or_page() {
        translate_l2(device, entry_to_table(&entry), guest_virtual_address)
    } else {
        let mask = (1 << 30) - 1;
        Some((entry.output_address().0 as u64 & !mask) | (guest_virtual_address & mask))
    }
}

fn translate_l2(
    device: &ModelDevice,
    table: &[Descriptor; 512],
    guest_virtual_address: u64,
) -> Option<u64> {
    let entry_idx = ((guest_virtual_address >> 21) & 0x1ff) as usize;
    log::trace!("l2 entry_idx: {entry_idx:x?}");
    let entry = &table[entry_idx];
    log::trace!("l2 entry: {entry:x?}");

    if !entry.is_valid() {
        exit_with_message!("invalid L2")
    }

    if entry.is_table_or_page() {
        translate_l3(device, entry_to_table(&entry), guest_virtual_address)
    } else {
        let mask = (1 << 21) - 1;
        Some((entry.output_address().0 as u64 & !mask) | (guest_virtual_address & mask))
    }
}

fn translate_l3(
    device: &ModelDevice,
    table: &[Descriptor; 512],
    guest_virtual_address: u64,
) -> Option<u64> {
    let entry_idx = ((guest_virtual_address >> 12) & 0x1ff) as usize;
    log::trace!("l3 entry_idx: {entry_idx:x?}");
    let entry = &table[entry_idx];
    log::trace!("l3 entry: {entry:x?}");

    if entry.is_table_or_page() {
        Some((entry.output_address().0 as u64) | (guest_virtual_address & ((1 << 12) - 1)))
    } else {
        log::warn!("invalid L3");
        guest_page_fault(device, guest_virtual_address);
        None
    }
}

fn entry_to_table(entry: &Descriptor) -> &[Descriptor; 512] {
    unsafe {
        &*guest_physical_to_host_virt(entry.output_address().0 as u64).as_ptr::<[Descriptor; 512]>()
    }
}

fn guest_page_fault(device: &ModelDevice, guest_virtual_address: u64) {
    log::warn!("guest page fault @ {guest_virtual_address}");

    let retaddr = device.register_file.read::<u64>("_PC");

    take_arm_exception(device, 1, 1, 0, guest_virtual_address, retaddr, 0);

    interrupt_restore_safepoint(1);
}

pub fn take_arm_exception(
    device: &ModelDevice,
    target_el: u8,
    typ: u8,
    syndrome: u32,
    vaddr: u64,
    retaddr: u64,
    mut voff: u64,
) {
    let spsr = get_psr_from_pstate(device);
    log::trace!("spsr: {spsr:032b}");

    let current_el = device.register_file.read::<u8>("PSTATE_EL");
    log::trace!("current_el: {current_el}");

    if target_el > current_el {
        voff += 0x400;
    } else if device.register_file.read::<u8>("PSTATE_SP") == 1 {
        voff += 0x200;
    }

    log::trace!("voff: {voff:x}");

    // Update the execution level
    device.register_file.write::<u8>("PSTATE_EL", target_el);

    // Update spsel
    device.register_file.write::<u8>("PSTATE_SP", 1);

    if target_el == 1 {
        device.register_file.write::<u32>("SPSR_EL1_bits", spsr);
        device.register_file.write::<u64>("ELR_EL1", retaddr);

        // If it's NOT an IRQ...
        if typ != 255 {
            let ec = get_exception_class(current_el, target_el, typ);
            device.register_file.write::<u32>(
                "ESR_EL1_bits",
                (ec << 26) | (1 << 25) | (syndrome & 0x1ffffff),
            );

            if typ == 1 || typ == 4 {
                device.register_file.write::<u64>("FAR_EL1", vaddr);
            }
        }
    } else {
        exit_with_message!("trap");
    }

    device.register_file.write::<u8>("PSTATE_D", 1);
    device.register_file.write::<u8>("PSTATE_A", 1);
    device.register_file.write::<u8>("PSTATE_I", 1);
    device.register_file.write::<u8>("PSTATE_F", 1);

    let vbar = device.register_file.read::<u64>(match current_el {
        1 => "VBAR_EL1",
        2 => "VBAR_EL2",
        3 => "VBAR_EL3",
        _ => exit_with_message!("invalid EL \"{current_el}\""),
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

fn get_exception_class(current_el: u8, target_el: u8, typ: u8) -> u32 {
    match typ {
        0 => {
            // Software Breakpoint
            0x38 + 4
        }
        1 => {
            if target_el == current_el {
                0x25
            } else {
                0x24
            }
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
            exit_with_message!("trap")
        }
    }
}
