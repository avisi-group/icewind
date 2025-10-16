#![no_std]
#![feature(abi_x86_interrupt)] // needed for interrupts
#![feature(allocator_api)] // needed for pci config regions and alignedallocator
#![feature(btree_cursors)]
#![feature(int_roundings)]
#![feature(btreemap_alloc)]
#![feature(iter_collect_into)]
#![feature(unsafe_cell_access)]
#![allow(static_mut_refs)] // todo: fix me

extern crate alloc;

use {
    crate::{
        guest::models,
        host::{
            arch::x86::memory::{
                HIGH_HALF_CANONICAL_END, HIGH_HALF_CANONICAL_START, PHYSICAL_MEMORY_OFFSET,
                VirtualMemoryArea,
            },
            devices::manager::SharedDeviceManager,
            fs::{Filesystem, tar::TarFilesystem},
            memory::bytes,
            rand, scheduler, tasks, timer,
        },
        logger::WRITER,
        util::try_get_current_device,
    },
    bootloader_api::{BootInfo, BootloaderConfig, config::Mapping},
    common::TestConfig,
    core::{panic::PanicInfo, sync::atomic::Ordering},
    x86::io::outw,
};

pub mod guest;
pub mod host;
pub mod logger;
pub mod tests;
pub mod util;

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


fn _serial_in() {
    let mut buf = [0u8; 64];

    loop {
        let read = unsafe { WRITER.get_mut() }
            .expect("WRITER not initialized")
            .read_bytes(&mut buf);

        if read > 0 {
            match core::str::from_utf8(&buf[..read]) {
                Ok(s) => match s {
                    "\u{3}" => {
                        log::error!("received Ctrl-C, terminating");
                        qemu_exit();
                    }
                    _ => log::debug!("{:?}", s),
                },
                Err(e) => log::error!("serial port received invalid UTF-8 {:?}", e),
            }
        }

        // todo nap time for a little bit
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    host::arch::x86::irq::local_disable();
    let (used, total) = host::arch::x86::memory::stats();

    log::error!("{info}");
    log::error!("heap {:.2}/{:.2} used", bytes(used), bytes(total));

    if let Some(device) = try_get_current_device() {
        log::error!(
            "Guest PC = {:#018x}, EL = {}",
            device.register_file.read::<u64>("_PC"),
            device.register_file.read::<u8>("PSTATE_EL")
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

/// Exits QEMU
fn qemu_exit() -> ! {
    unsafe { outw(0x604, 0x2000) };
    loop {
        x86_64::instructions::hlt();
    }
}
