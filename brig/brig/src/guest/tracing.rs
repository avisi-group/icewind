use {
    core::{
        fmt::Write,
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
    },
    kernel::devices::{Device, SharedDevice, manager::SharedDeviceManager},
    spin::Lazy,
};

pub static INSTRUCTION_COUNT: AtomicU64 = AtomicU64::new(0);
pub static ENABLED: AtomicBool = AtomicBool::new(false);

static CURRENT_TRACE_PACKET: Lazy<SharedDevice> = Lazy::new(|| {
    SharedDeviceManager::get()
        .get_device_by_alias("transport00:05.0")
        .unwrap()
});

pub extern "sysv64" fn trace_instruction_start(opcode: u32, pc: u64) {
    let count = INSTRUCTION_COUNT.fetch_add(1, Ordering::Relaxed);

    if ENABLED.load(Ordering::Relaxed) {
        let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
            panic!()
        };

        write!(transport, "{{ <{count}> [{pc:x}] ({opcode:x}) ").unwrap();
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
        if offset != 0x8820 {
            let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
                panic!()
            };
            write!(transport, "R[{offset:x}] => {value:#x}, ").unwrap();
        }
    }
}

pub extern "sysv64" fn trace_register_write(offset: u64, value: u64) {
    if ENABLED.load(Ordering::Relaxed) {
        if offset != 0x8820 {
            let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
                panic!()
            };
            write!(transport, "R[{offset:x}] <= {value:#x}, ").unwrap();
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
    // // if it's not in the low half
    // if !(address < 0x8000000000)
    //     // and outside of this range (presumably guest stack?)
    //     && !(0xffff_ffc0_0800_0000u64..0xffff_ffc0_0900_0000).contains(&address)
    //      && !(0xffff_fffe_0000_0000u64..0xffff_fffe_0100_0000u64).contains(&
    // address) {
    //     // log it
    //     log::error!("{address:x}");
    // }

    if ENABLED.load(Ordering::Relaxed) {
        let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
            panic!()
        };
        write!(transport, "M[{address:x}:{width}] <= {value:#x}, ").unwrap();
    }
}
