#![no_std]
#![no_main]
#![feature(allocator_api)]

extern crate alloc;

use {
    crate::guest::{linux_platform, models, run_guest, try_get_current_guest},
    bootloader_api::{BootInfo, BootloaderConfig, config::Mapping},
    common::{TestConfig, bytes},
    core::{panic::PanicInfo, sync::atomic::Ordering},
    kernel::{
        host::{
            self,
            arch::x86::memory::{
                HIGH_HALF_CANONICAL_END, HIGH_HALF_CANONICAL_START, PHYSICAL_MEMORY_OFFSET,
                VirtualMemoryArea,
            },
            devices::manager::SharedDeviceManager,
            fs::{Filesystem, tar::TarFilesystem},
            rand, scheduler, tasks, timer,
        },
        logger, qemu_exit,
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

    host::arch::CoreStorage::init_self();

    // required for generating UUIDs
    rand::init();

    // Host machine initialisation
    host::arch::platform_init(boot_info, page_fault_exception);
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
    // let serial_in_task = tasks::create_task(serial_in);
    // serial_in_task.start();

    let device_manager = SharedDeviceManager::get();
    let device = device_manager
        .get_device_by_alias("disk00:03.0")
        .expect("disk not found");

    let mut dev = device.lock();
    let mut fs = TarFilesystem::mount(dev.as_block());

    models::load_all(&mut fs);

    let test_config = {
        let file = fs
            .read_to_vec("test_config.postcard")
            .expect("failed to load test configuration file");
        postcard::from_bytes::<TestConfig>(&file).unwrap()
    };

    if test_config == TestConfig::None {
        // simbench_platform
        run_guest(linux_platform(), || guest::start(&mut fs));
    } else {
        run_guest(linux_platform(), || {
            common::tests::run(test_config);
            qemu_exit();
        });
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    host::arch::x86::irq::local_disable();
    let (used, total) = host::arch::x86::memory::stats();

    log::error!("{info}");
    log::error!("heap {:.2}/{:.2} used", bytes(used), bytes(total));

    if let Some(device) = try_get_current_guest() {
        log::error!(
            "Guest PC = {:#018x}, EL = {}",
            device.core.register_file.read::<u64>("_PC"),
            device.core.register_file.read::<u8>("PSTATE_EL")
        );

        log::error!(
            "Last executed opcode = {:#010x}",
            models::LAST_EXECUTED_OPCODE.load(Ordering::Relaxed)
        );
        log::error!(
            "Last translated opcode = {:#010x}",
            models::LAST_TRANSLATED_OPCODE.load(Ordering::Relaxed)
        );
    };

    // backtrace();

    qemu_exit();
}
