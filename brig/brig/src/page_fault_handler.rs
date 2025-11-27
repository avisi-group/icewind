use {
    crate::guest::{
        devices::arm::mmu::{TranslationType, guest_translate},
        get_current_guest,
    },
    alloc::sync::Arc,
    common::{
        GuestExecutionContext,
        device::MemoryMappedDevice,
        irq_handler,
        memory::{AddressSpace, AddressSpaceRegion, AddressSpaceRegionKind},
    },
    iced_x86::{Code, OpKind, Register},
    kernel::arch::x86::{
        MachineContext,
        memory::{
            GUEST_PHYSICAL_END, GUEST_PHYSICAL_START, LOW_HALF_CANONICAL_END,
            VIRT_GUEST_EMULATED_GUEST_PHYSICAL_START, VirtualMemoryArea,
        },
    },
    x86_64::{
        PhysAddr, VirtAddr,
        registers::control::Cr2,
        structures::{
            idt::PageFaultErrorCode,
            paging::{Page, PageTableFlags, PhysFrame, Size4KiB, Translate},
        },
    },
};

#[irq_handler(with_code = true)]
pub fn page_fault_exception(machine_context: *mut MachineContext) {
    let faulting_address = Cr2::read().unwrap();

    let machine_context = unsafe { &mut *machine_context };

    let error_code = PageFaultErrorCode::from_bits(machine_context.error_code).unwrap();

    let exec_ctx = GuestExecutionContext::current();
    let addrspace = unsafe { &*exec_ctx.current_address_space };

    const LOW_HALF: u64 = LOW_HALF_CANONICAL_END.as_u64();
    const EMULATED_GUEST_PHYS_START: u64 = GUEST_PHYSICAL_START.as_u64();
    const EMULATED_GUEST_PHYS_END: u64 = GUEST_PHYSICAL_END.as_u64();

    match faulting_address.as_u64() {
        ..LOW_HALF => handle_emulated_guest_virtual_memory_fault(
            faulting_address,
            error_code,
            machine_context,
            addrspace,
        ),
        EMULATED_GUEST_PHYS_START..EMULATED_GUEST_PHYS_END => {
            handle_emulated_guest_physical_memory_fault(faulting_address, error_code, addrspace)
        }

        _ => {
            panic!("HOST PAGE FAULT code {error_code:?} @ {faulting_address:?}");
        }
    }
}

fn allocate_and_map_emulated_guest_physical_memory(guest_physical: u64) -> PhysAddr {
    let backing_page =
        (VIRT_GUEST_EMULATED_GUEST_PHYSICAL_START + guest_physical).align_down(0x1000u64);

    VirtualMemoryArea::current().map_page(
        Page::<Size4KiB>::from_start_address(
            (GUEST_PHYSICAL_START + guest_physical).align_down(0x1000u64),
        )
        .unwrap(),
        PhysFrame::from_start_address(backing_page).unwrap(),
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
    );

    backing_page
}

fn handle_emulated_guest_io_access(
    guest_physical: u64,
    error_code: PageFaultErrorCode,
    rgn: &AddressSpaceRegion,
    device: Arc<dyn MemoryMappedDevice>,
    machine_context: &mut MachineContext,
) {
    let offset = guest_physical - rgn.base();

    let data = unsafe { &*(machine_context.rip as *const [u8; 15]) };

    let mut decoder = iced_x86::Decoder::new(64, data, 0);
    let faulting_instruction = decoder.decode();

    let is_write = error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE);

    if is_write {
        log::debug!("device write @ {offset:x} with instr {faulting_instruction:?}");

        let (value, size) = match faulting_instruction.op1_kind() {
            OpKind::Register => match faulting_instruction.op1_register() {
                Register::CL => (machine_context.rcx, 1),
                Register::CX => (machine_context.rcx, 2),
                Register::ECX => (machine_context.rcx, 4),
                Register::AL => (machine_context.rax, 1),
                Register::AX => (machine_context.rax, 2),
                Register::EAX => (machine_context.rax, 4),
                reg => {
                    panic!("todo write src reg {reg:?}")
                }
            },

            OpKind::Immediate8 => (faulting_instruction.immediate8() as u64, 1),
            OpKind::Immediate8_2nd => (faulting_instruction.immediate8_2nd() as u64, 1),
            OpKind::Immediate16 => (faulting_instruction.immediate16() as u64, 2),
            OpKind::Immediate32 => (faulting_instruction.immediate32() as u64, 4),
            OpKind::Immediate64 => (faulting_instruction.immediate64(), 8),
            OpKind::Immediate8to16 => (faulting_instruction.immediate8to16() as u64, 2),
            OpKind::Immediate8to32 => (faulting_instruction.immediate8to32() as u64, 4),
            OpKind::Immediate8to64 => (faulting_instruction.immediate8to64() as u64, 8),
            OpKind::Immediate32to64 => (faulting_instruction.immediate32to64() as u64, 8),

            kind => {
                panic!("device write todo op1 kind {kind:?}  {faulting_instruction:?}")
            }
        };

        let bytes = &value.to_le_bytes()[..size];

        log::debug!("writing {bytes:x?} to device @ {offset:x?}");

        device.write(offset, bytes);
    } else {
        // read
        // todo: refactor me
        let (dest, size) = match faulting_instruction.code() {
            Code::Mov_r32_rm32 => {
                let dest = faulting_instruction.op0_register();

                let size = if dest.is_gpr8() {
                    1
                } else if dest.is_gpr16() {
                    2
                } else if dest.is_gpr32() {
                    4
                } else if dest.is_gpr64() {
                    8
                } else {
                    panic!()
                };

                (dest, size)
            }
            Code::Mov_r16_rm16 => {
                let dest = faulting_instruction.op0_register();

                let size = if dest.is_gpr8() {
                    1
                } else if dest.is_gpr16() {
                    2
                } else if dest.is_gpr32() {
                    4
                } else if dest.is_gpr64() {
                    8
                } else {
                    panic!()
                };

                (dest, size)
            }
            code => {
                panic!("read code: {code:?}, instr: {faulting_instruction:?}")
            }
        };

        let mut bytes = alloc::vec![0; size ];

        device.read(offset, &mut bytes);

        log::debug!("read {bytes:x?} from device, writing to {dest:?}");

        // write bytes to dest

        match dest {
            Register::EAX => {
                let data = u32::from_le_bytes(bytes[0..4].try_into().unwrap());

                // set data (will zero top half of rax when we cast data
                // to a u64)
                machine_context.rax = data as u64;
            }
            Register::AX => {
                let data = u16::from_le_bytes(bytes[0..2].try_into().unwrap());

                machine_context.rax &= 0xFFFF_FFFF_FFFF_0000;
                machine_context.rax |= data as u64;
            }
            register => {
                panic!("register: {register:?}, data: {bytes:?}, instr: {faulting_instruction:?}")
            }
        }
    }

    // jump back to next instruction
    let current_ip = machine_context.rip;
    let len = faulting_instruction.len();

    machine_context.rip = current_ip + faulting_instruction.len() as u64;

    log::debug!(
        "setting correct return point: current_ip: {current_ip:x}, len: {len:x}, new_rip: {:x}",
        machine_context.rip
    );
}

fn handle_emulated_guest_virtual_memory_fault(
    faulting_address: VirtAddr,
    error_code: PageFaultErrorCode,
    machine_context: &mut MachineContext,
    addrspace: &AddressSpace,
) {
    log::debug!("guest fault @ {faulting_address:#x} ({error_code:?})");

    let is_write = error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE);

    // correct the address as it was masked off in emitter.rs:read/write-memory
    let unmasked_address = VirtAddr::new((((faulting_address.as_u64() as i64) << 24) >> 24) as u64);

    let guest_physical = guest_translate(
        &get_current_guest().core,
        unmasked_address.as_u64(),
        if is_write {
            TranslationType::Write
        } else {
            TranslationType::Read
        },
    );

    log::debug!("guest physical: {guest_physical:x?}");

    // ---------------------------------------------------------------------------------------- //
    let host_virtual_in_gp_mapping = (GUEST_PHYSICAL_START + guest_physical).align_down(0x1000u64);

    log::debug!("host virtual: {host_virtual_in_gp_mapping:x?}");

    let guest_backing_frame = VirtualMemoryArea::current()
        .opt
        .translate_addr(host_virtual_in_gp_mapping);

    log::debug!("guest backing frame: {guest_backing_frame:x?}");

    // have we already allocated this gues physical address?
    let backing_page = match guest_backing_frame {
        None => {
            // No existing backing page, so lookup what to do.
            if let Some(rgn) = addrspace.find_region(guest_physical) {
                // Physical address lies within a valid guest region, determine
                //region             type...
                match rgn.kind() {
                    AddressSpaceRegionKind::Ram => {
                        allocate_and_map_emulated_guest_physical_memory(guest_physical)
                    }
                    AddressSpaceRegionKind::IO(device) => {
                        handle_emulated_guest_io_access(
                            guest_physical,
                            error_code,
                            rgn,
                            device.clone(),
                            machine_context,
                        );

                        return;
                    }
                }
            } else {
                // Physical address not in valid guest region -- real fault.
                panic!(
                    "GUEST PAGE FAULT code {error_code:?} @ {guest_physical:x?}: no region -- this is a real fault, RIP: {:x}",
                    machine_context.rip
                )
            }
        }
        Some(phys_addr) => {
            // Backing page already exists at this host physical address
            phys_addr
        }
    };

    log::debug!(
        "guest backing page: {backing_page:x?} mapping to {:x?}",
        faulting_address.align_down(0x1000u64)
    );

    // ---------------------------------------------------------------------------------------- //
    // let backing_page =
    //     (VIRT_GUEST_EMULATED_GUEST_PHYSICAL_START +
    // guest_physical).align_down(0x1000u64);
    // ---------------------------------------------------------------------------------------- //
    let flags = if is_write {
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE
    } else {
        PageTableFlags::PRESENT
    };

    VirtualMemoryArea::current().map_page_propagate_invalidation(
        Page::<Size4KiB>::from_start_address(faulting_address.align_down(0x1000u64)).unwrap(),
        PhysFrame::from_start_address(backing_page).unwrap(),
        flags,
    );
}

fn handle_emulated_guest_physical_memory_fault(
    faulting_address: VirtAddr,
    error_code: PageFaultErrorCode,
    addrspace: &AddressSpace,
) {
    let guest_physical = faulting_address - GUEST_PHYSICAL_START;
    if let Some(rgn) = addrspace.find_region(guest_physical) {
        // Physical address lies within a valid guest region, determine region
        // type...
        match rgn.kind() {
            AddressSpaceRegionKind::Ram => {
                let backing_page = allocate_and_map_emulated_guest_physical_memory(guest_physical);

                log::debug!(
                    "assigned backing page {backing_page:x?} -> {:x?}",
                    (GUEST_PHYSICAL_START + guest_physical).align_down(0x1000u64)
                );
            }
            _ => {
                panic!(
                    "PAGE FAULT IN NON-RAM-BACKED GUEST PHYSICAL REGION code {error_code:?} @ {faulting_address:?}"
                );
            }
        }
    } else {
        panic!("PAGE FAULT IN GUEST PHYSICAL REGION code {error_code:?} @ {faulting_address:?}");
    }
}
