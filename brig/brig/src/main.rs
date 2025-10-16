#![no_std]
#![no_main]

use brig::BOOTLOADER_CONFIG;

bootloader_api::entry_point!(brig::start, config = &BOOTLOADER_CONFIG);
