use {
    crate::guest::{get_current_guest, models::get},
    common::GuestExecutionContext,
    core::{
        fmt::Write,
        sync::atomic::{AtomicBool, Ordering},
    },
    kernel::devices::{Device, SharedDevice, manager::SharedDeviceManager},
    spin::Lazy,
};

pub static ENABLED: AtomicBool = AtomicBool::new(false);

static CURRENT_TRACE_PACKET: Lazy<SharedDevice> = Lazy::new(|| {
    SharedDeviceManager::get()
        .get_device_by_alias("transport00:04.0")
        .unwrap()
});

pub extern "sysv64" fn trace_instruction_start(opcode: u32, pc: u64) {
    if ENABLED.load(Ordering::Relaxed) {
        let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
            panic!()
        };

        let count = GuestExecutionContext::current().instruction_count;

        write!(transport, "{{ <{count}> [{pc:x}] ({opcode:x}) ").unwrap();

        let pc = get_current_guest().core.well_known_registers.pc().read();

        // strlen_asimd
        if pc >= 0x417900 && pc < 0x417a3c {
            let z_offset = get_current_guest().core.model.reg_offset("_Z") as usize;

            let q0_offset = z_offset + 0 * 256;
            let q1_offset = z_offset + 1 * 256;
            let q2_offset = z_offset + 2 * 256;

            let q0 = get_current_guest()
                .core
                .register_file
                .read_raw::<u128>(q0_offset);
            let q1 = get_current_guest()
                .core
                .register_file
                .read_raw::<u128>(q1_offset);
            let q2 = get_current_guest()
                .core
                .register_file
                .read_raw::<u128>(q2_offset);

            write!(transport, "v0: {q0:032x}, v1: {q1:032x}, v2: {q2:032x} ",).unwrap();
        }
        //  writeln!(transport, "{pc:016x}").unwrap();
    }
}

pub extern "sysv64" fn trace_instruction_end() {
    if ENABLED.load(Ordering::Relaxed) {
        let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
            panic!()
        };

        writeln!(transport, " }}").unwrap();
    }
}

pub extern "sysv64" fn trace_register_read(offset: u64, value: u64) {
    if ENABLED.load(Ordering::Relaxed) {
        // PC and branchtaken registers
        if offset != 0x8820 && offset != 0x10d0 {
            let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
                panic!()
            };

            let name = get_current_guest()
                .core
                .model
                .get_register_by_offset(offset)
                .unwrap();

            write!(transport, "R[{name}] => {value:#x}, ").unwrap();
        }
    }
}

pub extern "sysv64" fn trace_register_write(offset: u64, value: u64) {
    if ENABLED.load(Ordering::Relaxed) {
        // PC and branchtaken registers
        if offset != 0x8820 && offset != 0x10d0 {
            let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
                panic!()
            };

            let name = get_current_guest()
                .core
                .model
                .get_register_by_offset(offset)
                .unwrap();

            write!(transport, "R[{name}] <= {value:#x}, ").unwrap();
        }
    }
}

pub extern "sysv64" fn trace_memory_read(address: u64, value: u64, width: u8) {
    if ENABLED.load(Ordering::Relaxed) {
        let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
            panic!()
        };
        write!(transport, "M[{address:x}:{width}] => {value:#x}, ").unwrap();
    }
}

pub extern "sysv64" fn trace_memory_write(address: u64, value: u64, width: u8) {
    if ENABLED.load(Ordering::Relaxed) {
        let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
            panic!()
        };
        write!(transport, "M[{address:x}:{width}] <= {value:#x}, ").unwrap();
    }
}
