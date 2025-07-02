use {
    crate::{
        guest::memory::AddressSpaceRegionKind,
        host::{
            arch::x86::{
                MachineContext,
                aarch64_mmu::guest_translate,
                dbg,
                memory::{
                    GUEST_PHYSICAL_START, LOW_HALF_CANONICAL_END, VirtAddrExt, VirtualMemoryArea,
                },
            },
            dbt::models::ModelDevice,
        },
        qemu_exit,
    },
    alloc::alloc::alloc_zeroed,
    bitset_core::BitSet,
    common::intern::InternedString,
    core::{alloc::Layout, any::Any},
    iced_x86::{Code, OpKind, Register},
    proc_macro_lib::irq_handler,
    spin::Once,
    x86::irq::{
        BREAKPOINT_VECTOR, DEBUG_VECTOR, DIVIDE_ERROR_VECTOR, DOUBLE_FAULT_VECTOR,
        GENERAL_PROTECTION_FAULT_VECTOR, PAGE_FAULT_VECTOR,
    },
    x86_64::{
        VirtAddr,
        registers::control::Cr2,
        structures::{
            idt::{InterruptDescriptorTable, PageFaultErrorCode},
            paging::{Page, PageTableFlags, PhysFrame, Size4KiB, Translate},
        },
    },
};

/// Print supplied message at `error` level, then exit QEMU
///
/// Panicking inside IRQ handler results in infinite loop and clutters log
/// output
macro_rules! exit_with_message {
    ($($arg:tt)*) => {
        (|| {
            log::error!($($arg)*);
            qemu_exit()
        })()
    }
}
pub(crate) use exit_with_message;

static mut IRQ_MANAGER: Once<IrqManager> = Once::INIT;

pub fn init() {
    unsafe {
        IRQ_MANAGER.call_once(|| IrqManager::new());
        let irqm = IRQ_MANAGER.get_mut().unwrap();
        irqm.setup().unwrap();
        irqm.idt.load();
    };
}

pub fn assign_irq(nr: u8, handler: IrqHandlerFn) -> Result<(), IrqError> {
    let irqm = unsafe { IRQ_MANAGER.get_mut() }.unwrap();
    irqm.assign_irq(nr, handler)?;
    irqm.idt.load();
    Ok(())
}

struct IrqManager {
    idt: InterruptDescriptorTable,
    used: UsedInterruptVectors,
}

impl IrqManager {
    fn new() -> Self {
        Self {
            idt: InterruptDescriptorTable::new(),
            used: UsedInterruptVectors::new(),
        }
    }

    fn setup(&mut self) -> Result<(), IrqError> {
        unsafe {
            // page fault
            self.idt
                .page_fault
                .set_handler_addr(VirtAddr::from_ptr(page_fault_exception as *const u8));
            self.used.set(PAGE_FAULT_VECTOR);

            // general protection
            self.idt
                .general_protection_fault
                .set_handler_addr(VirtAddr::from_ptr(gpf_exception as *const u8));
            self.used.set(GENERAL_PROTECTION_FAULT_VECTOR);

            // breakpoint
            self.idt
                .breakpoint
                .set_handler_addr(VirtAddr::from_ptr(breakpoint_exception as *const u8));
            self.used.set(BREAKPOINT_VECTOR);

            // breakpoint
            self.idt
                .debug
                .set_handler_addr(VirtAddr::from_ptr(debug_exception as *const u8));
            self.used.set(DEBUG_VECTOR);

            // double fault
            self.idt
                .double_fault
                .set_handler_addr(VirtAddr::from_ptr(double_fault_exception as *const u8));
            self.used.set(DOUBLE_FAULT_VECTOR);

            // double fault
            self.idt
                .divide_error
                .set_handler_addr(VirtAddr::from_ptr(div0_exception as *const u8));
            self.used.set(DIVIDE_ERROR_VECTOR);
        };

        for (f, i) in [
            (dbt_handler_undefined_terminator as IrqHandlerFn, 0x50),
            (dbt_handler_default_terminator, 0x51),
            (dbt_handler_const_assert, 0x52),
            (dbt_handler_panic, 0x53),
        ] {
            self.assign_irq(i, f)?;
        }

        Ok(())
    }

    fn assign_irq(&mut self, nr: u8, handler: IrqHandlerFn) -> Result<(), IrqError> {
        if !self.used.get(nr) {
            unsafe { self.idt[nr].set_handler_addr(VirtAddr::from_ptr(handler as *const u8)) };
            self.used.set(nr);
            Ok(())
        } else {
            Err(IrqError::IrqAlreadyReserved(nr))
        }
    }
}

/// IRQ Error
#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum IrqError {
    /// Attempted to assign IRQ {0} but it is already in use
    IrqAlreadyReserved(u8),
}

pub type IrqHandlerFn = unsafe extern "C" fn();

pub fn _local_enable() {
    x86_64::instructions::interrupts::enable();
}

pub fn local_disable() {
    x86_64::instructions::interrupts::disable();
}

#[irq_handler(with_code = true)]
fn page_fault_exception(machine_context: *mut MachineContext) {
    let faulting_address = Cr2::read().unwrap();

    let machine_context = unsafe { &mut *machine_context };

    let error_code = PageFaultErrorCode::from_bits(machine_context.error_code).unwrap();

    if faulting_address <= LOW_HALF_CANONICAL_END {
        log::debug!("guest fault @ {faulting_address:#x}");
        let exec_ctx = crate::guest::GuestExecutionContext::current();
        let addrspace = unsafe { &*exec_ctx.current_address_space };

        let device = ((&**unsafe {
            crate::guest::GUEST
                .get()
                .unwrap()
                .devices
                .get(&("core0".into()))
                .unwrap()
        }) as &dyn Any)
            .downcast_ref::<ModelDevice>()
            .unwrap();

        let pc = device.register_file.read::<u64>("_PC");
        log::debug!("PC = {pc:016x}");

        let mmu_enabled = device.register_file.read::<u64>("SCTLR_EL1_bits") & 1 == 1;

        // correct the address as it was masked off in emitter.rs:read/write-memory
        let unmasked_address =
            VirtAddr::new((((faulting_address.as_u64() as i64) << 24) >> 24) as u64);

        let guest_physical = if mmu_enabled {
            // translate:
            // * walk guest page tables from top level page table translate faulting address
            // * if it doesnt exist: guest page fault
            // * if it does exist but is invalid (write to a read only mapped page)
            // * or it works, we get a guest physical address, we do the next logic on line
            //   186 and map it as writeable, but if it was a read then map as read only
            // * map that guest physical address into the correct location in host virtual
            //   memory

            guest_translate(device, unmasked_address.as_u64()).unwrap()
        } else {
            unmasked_address.as_u64()
        };

        log::debug!("guest physical: {guest_physical:x?}");

        // gp = guest_physical
        let host_virtual_in_gp_mapping =
            (GUEST_PHYSICAL_START + guest_physical).align_down(0x1000u64);

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
                    // Physical address lies within a valid guest region, determine region type...
                    match rgn.kind() {
                        AddressSpaceRegionKind::Ram => {
                            // Physical address lies within a RAM-backed region, so allocate a
                            // backing page.
                            let backing_page = VirtAddr::from_ptr(unsafe {
                                alloc_zeroed(Layout::from_size_align(0x1000, 0x1000).unwrap())
                            })
                            .to_phys();

                            // Map the allocated backing page into the 1-1 guest phyical memory area
                            VirtualMemoryArea::current().map_page(
                                Page::<Size4KiB>::from_start_address(
                                    (GUEST_PHYSICAL_START + guest_physical).align_down(0x1000u64),
                                )
                                .unwrap(),
                                PhysFrame::from_start_address(backing_page).unwrap(),
                                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                            );

                            log::debug!(
                                "allocated backing page {backing_page:x?} -> {:x?}",
                                (GUEST_PHYSICAL_START + guest_physical).align_down(0x1000u64)
                            );

                            backing_page
                        }
                        AddressSpaceRegionKind::IO(device) => {
                            log::debug!("guest device page fault at rip {:x}", machine_context.rip);

                            let offset = guest_physical - rgn.base();

                            let write = error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE);

                            let data = unsafe { &*(machine_context.rip as *const [u8; 15]) };

                            let mut decoder = iced_x86::Decoder::new(64, data, 0);
                            let faulting_instruction = decoder.decode();

                            if write {
                                log::debug!(
                                    "device write @ {offset:x} with instr {faulting_instruction:?}"
                                );

                                let (value, size) = match faulting_instruction.op1_kind() {
                                    OpKind::Register => match faulting_instruction.op1_register() {
                                        Register::CL => (machine_context.rcx, 1),
                                        Register::ECX => (machine_context.rcx, 4),
                                        reg => {
                                            exit_with_message!("todo write src reg {reg:?}")
                                        }
                                    },

                                    OpKind::Immediate8 => {
                                        (faulting_instruction.immediate8() as u64, 1)
                                    }
                                    OpKind::Immediate8_2nd => {
                                        (faulting_instruction.immediate8_2nd() as u64, 1)
                                    }
                                    OpKind::Immediate16 => {
                                        (faulting_instruction.immediate16() as u64, 2)
                                    }
                                    OpKind::Immediate32 => {
                                        (faulting_instruction.immediate32() as u64, 4)
                                    }
                                    OpKind::Immediate64 => (faulting_instruction.immediate64(), 8),
                                    OpKind::Immediate8to16 => {
                                        (faulting_instruction.immediate8to16() as u64, 2)
                                    }
                                    OpKind::Immediate8to32 => {
                                        (faulting_instruction.immediate8to32() as u64, 4)
                                    }
                                    OpKind::Immediate8to64 => {
                                        (faulting_instruction.immediate8to64() as u64, 8)
                                    }
                                    OpKind::Immediate32to64 => {
                                        (faulting_instruction.immediate32to64() as u64, 8)
                                    }

                                    kind => {
                                        exit_with_message!(
                                            "device write todo op1 kind {kind:?}  {faulting_instruction:?}"
                                        )
                                    }
                                };

                                // let (value, size) = match faulting_instruction.code() {
                                //     Code::Mov_rm8_r8 => match src {
                                //         Register::CL => (machine_context.rcx, 1),
                                //         reg => {
                                //             exit_with_message!("todo write src reg {reg:?}")
                                //         }
                                //     },
                                //    => {

                                //     }
                                //     Code::Mov_rm32_imm32 => {
                                //         (faulting_instruction.immediate32() as u64, 4)
                                //     }

                                //     code => {
                                //         exit_with_message!(
                                //             "write code: {code:?}, instr:
                                // {faulting_instruction:?}"
                                //         )
                                //     }
                                // };

                                let bytes = &value.to_le_bytes()[..size];

                                log::debug!("writing {bytes:x?} to device @ {offset:x?}");

                                device.write(offset, bytes);
                            } else {
                                // read
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
                                    code => {
                                        exit_with_message!(
                                            "read code: {code:?}, instr: {faulting_instruction:?}"
                                        )
                                    }
                                };

                                let mut bytes = alloc::vec![0; size ];

                                device.read(offset, &mut bytes);

                                log::debug!("read {bytes:x?} from device, writing to {dest:?}");

                                // write bytes to dest

                                match dest {
                                    Register::EAX => {
                                        let data =
                                            u32::from_le_bytes(bytes[0..4].try_into().unwrap());
                                        // mask
                                        machine_context.rax &= 0xFFFF_FFFF_0000_0000;
                                        // or in data
                                        machine_context.rax |= data as u64;
                                    }
                                    register => {
                                        exit_with_message!(
                                            "register: {register:?}, data: {bytes:?}, instr: {faulting_instruction:?}"
                                        )
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

                            return;
                        }
                    }
                } else {
                    // Physical address not in valid guest region -- real fault.
                    exit_with_message!(
                        "GUEST PAGE FAULT code {error_code:?} @ {guest_physical:x?}: no region -- this is a real fault"
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

        VirtualMemoryArea::current().map_page_propagate_invalidation(
            Page::<Size4KiB>::from_start_address(faulting_address.align_down(0x1000u64)).unwrap(),
            PhysFrame::from_start_address(backing_page).unwrap(),
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
        );
    } else {
        exit_with_message!("HOST PAGE FAULT code {error_code:?} @ {faulting_address:?}");
    }
}

#[irq_handler(with_code = false)]
fn div0_exception() {
    exit_with_message!("EXCEPTION: DIVIDE BY 0");
}

#[irq_handler(with_code = false)]
fn breakpoint_exception() {
    exit_with_message!("EXCEPTION: BREAKPOINT");
}

#[irq_handler(with_code = false)]
fn debug_exception() {
    dbg::handle_exception();
}

#[irq_handler(with_code = true)]
fn double_fault_exception() {
    exit_with_message!("EXCEPTION: DOUBLE-FAULT");
}

#[irq_handler(with_code = true)]
fn gpf_exception(machine_context: *mut MachineContext) {
    exit_with_message!(
        "EXCEPTION: GENERAL PROTECTION FAULT\nrip = {:x}",
        unsafe { &*machine_context }.rip
    );
}

#[irq_handler(with_code = true)]
fn dbt_handler_undefined_terminator(_machine_context: *mut MachineContext) {
    exit_with_message!("DBT interrupt: undefined terminator")
}

#[irq_handler(with_code = true)]
fn dbt_handler_default_terminator(_machine_context: *mut MachineContext) {
    exit_with_message!("DBT interrupt: default terminator")
}

#[irq_handler(with_code = true)]
fn dbt_handler_const_assert(_machine_context: *mut MachineContext) {
    exit_with_message!("DBT interrupt: const assert")
}

#[irq_handler(with_code = true)]
fn dbt_handler_panic(machine_context: *mut MachineContext) {
    let meta = unsafe { &*machine_context }.r15;

    let key = (meta >> 32) as u32;
    let function_name = InternedString::from_raw(key - 1);

    let block = (meta >> 16) as u16;
    let statement = meta as u16;

    exit_with_message!(
        "DBT interrupt: statement {statement:x} failed assert in block {block:x} of {function_name:?}"
    )
}

struct UsedInterruptVectors([u64; 4]);

impl UsedInterruptVectors {
    pub fn new() -> Self {
        Self([0; 4])
    }

    pub fn set(&mut self, nr: u8) {
        self.0.bit_set(usize::from(nr));
    }

    pub fn get(&mut self, nr: u8) -> bool {
        self.0.bit_test(usize::from(nr))
    }
}
