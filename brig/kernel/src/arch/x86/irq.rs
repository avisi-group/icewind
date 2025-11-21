use {
    crate::arch::x86::{MachineContext, dbg},
    bitset_core::BitSet,
    common::{intern::InternedString, irq_handler},
    spin::Once,
    x86::irq::{
        BREAKPOINT_VECTOR, DEBUG_VECTOR, DIVIDE_ERROR_VECTOR, DOUBLE_FAULT_VECTOR,
        GENERAL_PROTECTION_FAULT_VECTOR, PAGE_FAULT_VECTOR,
    },
    x86_64::{VirtAddr, structures::idt::InterruptDescriptorTable},
};

pub static mut IRQ_MANAGER: Once<IrqManager> = Once::INIT;

pub fn init(page_fault_exception: unsafe extern "C" fn()) {
    unsafe {
        IRQ_MANAGER.call_once(|| IrqManager::new());
        let irqm = IRQ_MANAGER.get_mut().unwrap();
        irqm.setup(page_fault_exception).unwrap();
        irqm.idt.load();
    };
}

pub fn assign_irq(nr: u8, handler: IrqHandlerFn) -> Result<(), IrqError> {
    let irqm = unsafe { IRQ_MANAGER.get_mut() }.unwrap();
    irqm.assign_irq(nr, handler)?;
    irqm.idt.load();
    Ok(())
}

pub struct IrqManager {
    pub idt: InterruptDescriptorTable,
    used: UsedInterruptVectors,
}

impl IrqManager {
    fn new() -> Self {
        Self {
            idt: InterruptDescriptorTable::new(),
            used: UsedInterruptVectors::new(),
        }
    }

    fn setup(&mut self, page_fault_exception: unsafe extern "C" fn()) -> Result<(), IrqError> {
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

#[irq_handler(with_code = false)]
fn div0_exception() {
    panic!("EXCEPTION: DIVIDE BY 0");
}

#[irq_handler(with_code = false)]
fn breakpoint_exception() {
    panic!("EXCEPTION: BREAKPOINT");
}

#[irq_handler(with_code = false)]
fn debug_exception() {
    dbg::handle_exception();
}

#[irq_handler(with_code = true)]
fn double_fault_exception() {
    panic!("EXCEPTION: DOUBLE-FAULT");
}

#[irq_handler(with_code = true)]
fn gpf_exception(machine_context: *mut MachineContext) {
    panic!("EXCEPTION: GENERAL PROTECTION FAULT\n{:#x?}", unsafe {
        &*machine_context
    });
}

#[irq_handler(with_code = true)]
fn dbt_handler_undefined_terminator(_machine_context: *mut MachineContext) {
    panic!("DBT interrupt: undefined terminator")
}

#[irq_handler(with_code = true)]
fn dbt_handler_default_terminator(_machine_context: *mut MachineContext) {
    panic!("DBT interrupt: default terminator")
}

#[irq_handler(with_code = true)]
fn dbt_handler_const_assert(_machine_context: *mut MachineContext) {
    panic!("DBT interrupt: const assert")
}

#[irq_handler(with_code = true)]
fn dbt_handler_panic(machine_context: *mut MachineContext) {
    let meta = unsafe { &*machine_context }.r15;

    let key = (meta >> 32) as u32;
    let function_name = InternedString::from_raw(key - 1);

    let block = (meta >> 16) as u16;
    let statement = meta as u16;

    panic!(
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
