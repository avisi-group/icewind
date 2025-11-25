use {
    ::x86::msr::{IA32_TSC_AUX, rdmsr, wrmsr},
    alloc::boxed::Box,
    bootloader_api::BootInfo,
    common::hashmap::HashMap,
    core::{
        any::{Any, TypeId},
        sync::atomic::{AtomicU64, Ordering},
    },
    log::trace,
    spin::Once,
};

pub mod x86;

pub const PAGE_SIZE: usize = 4096;

/// Platform initialization, triggers device enumeration and
/// initialization
pub fn platform_init(
    boot_info: &BootInfo,
    page_fault_exception: unsafe extern "C" fn(),
    continuation: extern "C" fn(u64),
) {
    trace!("initializing platform");
    x86::init(boot_info, page_fault_exception, continuation);
}

#[derive(Default)]
pub struct CoreStorage {
    state: HashMap<TypeId, Box<dyn Any>>,
}

static mut NEXT_CORE_ID: AtomicU64 = AtomicU64::new(0);
static mut CORES: [Once<CoreStorage>; 4] = [Once::INIT; 4];

fn get_local_pid() -> u64 {
    unsafe { rdmsr(IA32_TSC_AUX) }
}

impl CoreStorage {
    pub fn init_self() {
        unsafe { wrmsr(IA32_TSC_AUX, NEXT_CORE_ID.fetch_add(1, Ordering::SeqCst)) };

        trace!("initializing core {}", get_local_pid());
        unsafe { CORES[get_local_pid() as usize].call_once(Self::default) };
    }

    pub fn this_mut() -> &'static mut Self {
        unsafe { CORES[get_local_pid() as usize].get_mut().unwrap() }
    }

    pub fn set<O: 'static>(&mut self, o: O) {
        let key = TypeId::of::<O>();
        let value = Box::new(o);
        self.state.insert(key, value);
    }

    pub fn get<O: 'static>(&mut self) -> Option<&mut O> {
        let key = TypeId::of::<O>();
        self.state
            .get_mut(&key)
            .and_then(|any| any.downcast_mut::<O>())
    }
}
