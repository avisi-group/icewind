use {
    crate::guest::get_current_guest,
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
        .get_device_by_alias("transport00:05.0")
        .unwrap()
});

pub extern "sysv64" fn trace_instruction_start(opcode: u32, pc: u64) {
    if ENABLED.load(Ordering::Relaxed) {
        let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
            panic!()
        };

        let count = GuestExecutionContext::current().instruction_count;

        write!(transport, "{{ <{count}> [{pc:x}] ({opcode:x}) ").unwrap();
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
    // if ENABLED.load(Ordering::Relaxed) {
    //     // PC and branchtaken registers
    //     if offset != 0x8820 && offset != 0x10d0 {
    //         let Device::Transport(transport) = &mut
    // *CURRENT_TRACE_PACKET.lock() else {             panic!()
    //         };

    //         let name = get_current_guest()
    //             .core
    //             .model
    //             .get_register_by_offset(offset)
    //             .unwrap();

    //         write!(transport, "R[{name}] => {value:#x}, ").unwrap();
    //     }
    // }
}

pub extern "sysv64" fn trace_register_write(offset: u64, value: u64) {
    // if ENABLED.load(Ordering::Relaxed) {
    //     // PC and branchtaken registers
    //     if offset != 0x8820 && offset != 0x10d0 {
    //         let Device::Transport(transport) = &mut
    // *CURRENT_TRACE_PACKET.lock() else {             panic!()
    //         };

    //         let name = get_current_guest()
    //             .core
    //             .model
    //             .get_register_by_offset(offset)
    //             .unwrap();

    //         write!(transport, "R[{name}] <= {value:#x}, ").unwrap();
    //     }
    // }
}

pub extern "sysv64" fn trace_memory_read(address: u64, value: u64, width: u8) {
    // if ENABLED.load(Ordering::Relaxed) {
    //     let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock()
    // else {         panic!()
    //     };
    //     write!(transport, "M[{address:x}:{width}] => {value:#x}, ").unwrap();
    // }
}

pub extern "sysv64" fn trace_memory_write(address: u64, value: u64, width: u8) {
    // if ENABLED.load(Ordering::Relaxed) {
    //     let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock()
    // else {         panic!()
    //     };
    //     write!(transport, "M[{address:x}:{width}] <= {value:#x}, ").unwrap();
    // }
}
