#![no_std]
#![no_main]
#![feature(allocator_api)]

extern crate alloc;

use {
    crate::guest::{activate_guest_context, linux_platform, models, try_get_current_guest},
    bootloader_api::{BootInfo, BootloaderConfig, config::Mapping},
    common::{TestConfig, bytes},
    core::{panic::PanicInfo, sync::atomic::Ordering},
    kernel::{
        arch::{
            self,
            x86::memory::{
                HIGH_HALF_CANONICAL_END, HIGH_HALF_CANONICAL_START, PHYSICAL_MEMORY_OFFSET,
                VirtualMemoryArea,
            },
        },
        devices::virtio::{self, block::DeviceFunction},
        fs::{Filesystem, tar::TarFilesystem},
        logger, qemu_exit, rand, scheduler, tasks, timer,
    },
    page_fault_handler::page_fault_exception,
};

pub mod guest;
pub mod page_fault_handler;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::FixedAddress(PHYSICAL_MEMORY_OFFSET.as_u64()));
    config.mappings.dynamic_range_start = Some(HIGH_HALF_CANONICAL_START.as_u64());
    config.mappings.dynamic_range_end = Some(HIGH_HALF_CANONICAL_END.as_u64());
    config.mappings.framebuffer = Mapping::Dynamic;
    config.mappings.kernel_stack = Mapping::Dynamic;
    config.mappings.ramdisk_memory = Mapping::Dynamic;
    config.mappings.boot_info = Mapping::Dynamic;
    config.mappings.aslr = false;
    config.kernel_stack_size = 0x10_0000;
    config
};

pub fn start(boot_info: &'static mut BootInfo) -> ! {
    // note: logging device initialized internally before platform
    logger::init();

    VirtualMemoryArea::current().opt.level_4_table_mut()[0].set_unused();

    // required for generating UUIDs
    rand::init();

    // Host machine initialisation, starts virtual machine with continuation
    // function
    arch::platform_init(boot_info, page_fault_exception, continuation);

    unreachable!();
}

extern "C" fn continuation(rsdp_addr: u64) {
    log::trace!("VM continuation");

    // initialize device manager ready to register detected devices
    kernel::devices::manager::init();

    // probe system bus, this bootstraps device enumeration and initialization
    log::trace!("Probing system bus");
    kernel::arch::x86::system_bus::probe(rsdp_addr);

    arch::CoreStorage::init_self();

    timer::init();
    tasks::init();

    // occurs per core
    tasks::register_scheduler();

    {
        let continue_start_task = tasks::create_task(continue_start);
        continue_start_task.start();
    }

    scheduler::local_run();
}

fn continue_start() {
    log::debug!("continue start");

    // let serial_in_task = tasks::create_task(serial_in);
    // serial_in_task.start();
    let mut block = virtio::block::get(&DeviceFunction {
        bus: 0,
        device: 3,
        function: 0,
    });
    log::debug!("mounting tarfs");
    let mut fs = TarFilesystem::mount(&mut block);

    models::load_all(&mut fs);

    let test_config = {
        let file = fs
            .read_to_vec("test_config.postcard")
            .expect("failed to load test configuration file");
        postcard::from_bytes::<TestConfig>(&file).unwrap()
    };

    // alternatively simbench_platform
    let guest = linux_platform();
    activate_guest_context(guest);

    if test_config == TestConfig::None {
        guest::start(&mut fs);
    } else {
        common::tests::run(test_config);
        qemu_exit();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    arch::x86::irq::local_disable();
    let (used, total) = arch::x86::memory::stats();

    log::error!("{info}");
    log::error!("heap {:.2}/{:.2} used", bytes(used), bytes(total));

    if let Some(device) = try_get_current_guest() {
        let guest_pc = device.core.register_file.read::<u64>("_PC");
        log::error!(
            "Guest PC = {guest_pc:#018x} ({:#010x}), EL = {}",
            unsafe { *(guest_pc as *const u32) },
            device.core.register_file.read::<u8>("PSTATE_EL")
        );

        log::error!(
            "Last translated opcode = {:#010x}",
            models::LAST_TRANSLATED_OPCODE.load(Ordering::Relaxed)
        );
    };

    // backtrace();

    qemu_exit();
}
