#![no_std]
#![no_main]

use {
    bootloader_api::{BootInfo, BootloaderConfig, config::Mapping},
    common::TestConfig,
    kernel::{
        guest::{self, models},
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
        logger,
    },
};

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
    host::arch::platform_init(boot_info);
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
        guest::start(&mut fs);
    } else {
        guest::tests(test_config)
    }
}
