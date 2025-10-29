use {
    alloc::vec::Vec,
    core::{
        fmt::{self, Display, Write},
        sync::atomic::AtomicU64,
    },
    kernel::devices::{Device, SharedDevice, TransportDevice, manager::SharedDeviceManager},
    spin::{Lazy, Mutex},
    x86::io::outb,
};

static INSTRUCTION_COUNT: AtomicU64 = AtomicU64::new(0);

static CURRENT_TRACE_PACKET: Lazy<SharedDevice> = Lazy::new(|| {
    SharedDeviceManager::get()
        .get_device_by_alias("transport00:05.0")
        .unwrap()
});

pub extern "sysv64" fn trace_instruction_start(opcode: u32, pc: u64) {
    let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
        panic!()
    };

    let count = INSTRUCTION_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    write!(transport, "{{ <{count}> [{pc:x}] ({opcode:x}) ").unwrap();
}

pub extern "sysv64" fn trace_instruction_end() {
    let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
        panic!()
    };

    writeln!(transport, " }}").unwrap();
}

pub extern "sysv64" fn trace_register_read(offset: u64, value: u64) {
    if offset != 0x8820 {
        let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
            panic!()
        };
        write!(transport, "R[{offset:x}] => {value:#x}, ").unwrap();
    }
}

pub extern "sysv64" fn trace_register_write(offset: u64, value: u64) {
    if offset != 0x8820 {
        let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
            panic!()
        };
        write!(transport, "R[{offset:x}] <= {value:#x}, ").unwrap();
    }
}

pub extern "sysv64" fn trace_memory_read(address: u64, value: u64, width: u8) {
    let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
        panic!()
    };
    write!(transport, "M[{address:x}:{width}] => {value:#x}, ").unwrap();
}

pub extern "sysv64" fn trace_memory_write(address: u64, value: u64, width: u8) {
    let Device::Transport(transport) = &mut *CURRENT_TRACE_PACKET.lock() else {
        panic!()
    };
    write!(transport, "M[{address:x}:{width}] <= {value:#x}, ").unwrap();
}
