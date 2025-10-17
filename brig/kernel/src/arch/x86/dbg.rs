use x86_64::{
    VirtAddr,
    registers::debug::{
        BreakpointCondition, BreakpointSize, DebugAddressRegister, DebugAddressRegisterNumber, Dr0,
        Dr1, Dr2, Dr3, Dr6, Dr7, Dr7Flags,
    },
};

static mut WATCHPOINTS: [Watchpoint; 4] = [
    Watchpoint::new(DebugAddressRegisterNumber::Dr0),
    Watchpoint::new(DebugAddressRegisterNumber::Dr1),
    Watchpoint::new(DebugAddressRegisterNumber::Dr2),
    Watchpoint::new(DebugAddressRegisterNumber::Dr3),
];

pub fn init() {}

pub fn handle_exception() {
    let watchpoint = get_watchpoint_from_dr6();

    if !watchpoint.valid {
        log::warn!("hit invalid watchpoint: {:x?}", watchpoint);
        return;
    }

    watchpoint.disable();

    log::error!("hit watchpoint: {watchpoint:x?} = {:016x}", unsafe {
        watchpoint.address.as_ptr::<u64>().read_unaligned()
    });

    watchpoint.enable();
}

fn get_watchpoint_from_dr6() -> &'static Watchpoint {
    let dr6 = Dr6::read();
    unsafe {
        match dr6.bits() & 0xF {
            1 => &WATCHPOINTS[0],
            2 => &WATCHPOINTS[1],
            4 => &WATCHPOINTS[2],
            8 => &WATCHPOINTS[3],
            _ => panic!("error multiple watchpoints asserted"),
        }
    }
}

pub fn add_memory_watchpoint(
    address: VirtAddr,
    size: BreakpointSize,
    condition: BreakpointCondition,
) {
    let watchpoint = unsafe { WATCHPOINTS.iter_mut() }
        .find(|w| !w.valid)
        .expect("no available watchpoints");

    watchpoint.address = address;
    watchpoint.size = size;
    watchpoint.condition = condition;
    watchpoint.valid = true;

    watchpoint.sync();
}

#[derive(Debug)]
pub struct Watchpoint {
    debug_register: DebugAddressRegisterNumber,
    valid: bool,
    address: VirtAddr,
    size: BreakpointSize,
    condition: BreakpointCondition,
}

impl Watchpoint {
    const fn new(debug_register: DebugAddressRegisterNumber) -> Self {
        Self {
            debug_register,
            valid: false,
            address: VirtAddr::zero(),
            size: BreakpointSize::Length1B,
            condition: BreakpointCondition::InstructionExecution,
        }
    }

    fn enable(&self) {
        let mut dr7 = Dr7::read();

        match self.debug_register {
            DebugAddressRegisterNumber::Dr0 => dr7.insert_flags(
                Dr7Flags::GLOBAL_BREAKPOINT_0_ENABLE | Dr7Flags::LOCAL_BREAKPOINT_0_ENABLE,
            ),
            DebugAddressRegisterNumber::Dr1 => dr7.insert_flags(
                Dr7Flags::GLOBAL_BREAKPOINT_1_ENABLE | Dr7Flags::LOCAL_BREAKPOINT_1_ENABLE,
            ),
            DebugAddressRegisterNumber::Dr2 => dr7.insert_flags(
                Dr7Flags::GLOBAL_BREAKPOINT_2_ENABLE | Dr7Flags::LOCAL_BREAKPOINT_2_ENABLE,
            ),
            DebugAddressRegisterNumber::Dr3 => dr7.insert_flags(
                Dr7Flags::GLOBAL_BREAKPOINT_3_ENABLE | Dr7Flags::LOCAL_BREAKPOINT_3_ENABLE,
            ),
        }

        Dr7::write(dr7);
    }

    fn disable(&self) {
        let mut dr7 = Dr7::read();

        match self.debug_register {
            DebugAddressRegisterNumber::Dr0 => dr7.remove_flags(
                Dr7Flags::GLOBAL_BREAKPOINT_0_ENABLE | Dr7Flags::LOCAL_BREAKPOINT_0_ENABLE,
            ),
            DebugAddressRegisterNumber::Dr1 => dr7.remove_flags(
                Dr7Flags::GLOBAL_BREAKPOINT_1_ENABLE | Dr7Flags::LOCAL_BREAKPOINT_1_ENABLE,
            ),
            DebugAddressRegisterNumber::Dr2 => dr7.remove_flags(
                Dr7Flags::GLOBAL_BREAKPOINT_2_ENABLE | Dr7Flags::LOCAL_BREAKPOINT_2_ENABLE,
            ),
            DebugAddressRegisterNumber::Dr3 => dr7.remove_flags(
                Dr7Flags::GLOBAL_BREAKPOINT_3_ENABLE | Dr7Flags::LOCAL_BREAKPOINT_3_ENABLE,
            ),
        }

        Dr7::write(dr7);
    }

    pub fn sync(&self) {
        if self.valid {
            let mut dr7 = Dr7::read();
            dr7.set_condition(self.debug_register, self.condition);
            dr7.set_size(self.debug_register, self.size);
            Dr7::write(dr7);

            match self.debug_register {
                DebugAddressRegisterNumber::Dr0 => Dr0::write(self.address.as_u64()),
                DebugAddressRegisterNumber::Dr1 => Dr1::write(self.address.as_u64()),
                DebugAddressRegisterNumber::Dr2 => Dr2::write(self.address.as_u64()),
                DebugAddressRegisterNumber::Dr3 => Dr3::write(self.address.as_u64()),
            }

            self.enable();
        } else {
            self.disable();
        }
    }
}
