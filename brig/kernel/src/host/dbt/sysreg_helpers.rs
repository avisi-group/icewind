use {
    crate::host::{
        arch::x86::aarch64_mmu::{AT_S1E1R, at_s1e1r_handler},
        objects::device::RegisterMappedDevice,
    },
    alloc::sync::Arc,
    common::hashmap::HashMap,
    spin::{Lazy, Mutex},
};

pub const fn encode_sysreg_id(op0: u64, op1: u64, crn: u64, crm: u64, op2: u64) -> u64 {
    (op0 << 19) | (op1 << 16) | (crn << 12) | (crm << 8) | (op2 << 5)
}

enum Handler {
    Device(Arc<dyn RegisterMappedDevice>),
    Fn(fn(u64)),
    // ttbr0
}

static SYSREG_HANDLERS: Lazy<Mutex<HashMap<u64, Handler>>> = Lazy::new(|| {
    let mut map = HashMap::default();
    map.insert(AT_S1E1R, Handler::Fn(at_s1e1r_handler));
    Mutex::new(map)
});

pub fn register_device(id: u64, device: Arc<dyn RegisterMappedDevice>) {
    SYSREG_HANDLERS.lock().insert(id, Handler::Device(device));
}

pub fn handler_exists(reg: u64) -> bool {
    SYSREG_HANDLERS.lock().contains_key(&reg)
}

pub fn sys_reg_read(reg: u64) -> u64 {
    let guard = SYSREG_HANDLERS.lock();
    let handler = guard.get(&reg).unwrap();

    let mut result = [0u8; 8];

    match handler {
        Handler::Device(dev) => dev.read(reg, &mut result),
        Handler::Fn(_) => todo!(),
    }

    u64::from_le_bytes(result)
}

pub fn sys_reg_write(reg: u64, value: u64, len: u8) {
    let guard = SYSREG_HANDLERS.lock();
    let handler = guard.get(&reg).unwrap();

    // TODO: 'len'
    match handler {
        Handler::Device(dev) => dev.write(reg, value.to_le_bytes().as_slice()),
        Handler::Fn(f) => f(value),
    }
}
