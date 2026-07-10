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

use {crate::logger::WRITER, x86::io::outw};

pub mod arch;
pub mod devices;
pub mod fs;
pub mod logger;
pub mod memory;
pub mod rand;
pub mod scheduler;
pub mod tasks;
pub mod timer;
pub mod util;

#[derive(Debug)]
pub enum StalePageMode {
    EPT,
    None,
    SoftwareTargetedFlush,
    SoftwareFullFlush,
}

pub const STALE_PAGE_MODE: StalePageMode = StalePageMode::EPT;

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

/// Exits QEMU
pub fn qemu_exit() -> ! {
    unsafe { outw(0x604, 0x2000) };
    loop {
        x86_64::instructions::hlt();
    }
}
