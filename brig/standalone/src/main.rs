use {
    aarch64_interpreter::{Aarch64Interpreter, TracerKind},
    rustix::mm::{MapFlags, ProtFlags},
};

const DTB: &[u8] = include_bytes!("../../../borealis/data/sail-arm/arm-v9.4-a/sail.dtb");
const BOOTLOADER: &[u8] =
    include_bytes!("../../../borealis/data/sail-arm/arm-v9.4-a/bootloader.bin");
const IMAGE: &[u8] = include_bytes!("../../brig-cli/guest_data/Image");

mod logger;

fn main() {
    logger::init();

    // create guest virtual memory?
    // from sail.dtb
    let _mmap0 = unsafe {
        rustix::mm::mmap_anonymous(
            0x8000_0000 as *mut _, // guest memory base address
            0x900_0000,            // guest memory size
            ProtFlags::READ | ProtFlags::WRITE,
            MapFlags::FIXED | MapFlags::PRIVATE,
        )
    }
    .unwrap();

    // map 4K at 0xdeadbed0 for the 0xdeadbeef stack pointer in the bootloader
    let _mmap_deadbed0 = unsafe {
        rustix::mm::mmap_anonymous(
            0xdead_b000 as *mut _,
            0x1000,
            ProtFlags::READ | ProtFlags::WRITE,
            MapFlags::FIXED | MapFlags::PRIVATE,
        )
    }
    .unwrap();

    //??
    let _mmap1 = unsafe {
        rustix::mm::mmap_anonymous(
            0x13000000 as *mut _,
            0x10_0000,
            ProtFlags::READ | ProtFlags::WRITE,
            MapFlags::FIXED | MapFlags::PRIVATE,
        )
    }
    .unwrap();

    let _gic = unsafe {
        rustix::mm::mmap_anonymous(
            0x2c00_0000 as *mut _,
            0x1_0000,
            ProtFlags::READ | ProtFlags::WRITE,
            MapFlags::FIXED | MapFlags::PRIVATE,
        )
    }
    .unwrap();

    // -b 0x80000000,bootloader.bin -b 0x81000000,sail.dtb -b 0x82080000,Image

    unsafe {
        // copy bootloader
        write_ram(BOOTLOADER, 0x8000_0000);
        // copy dtb
        write_ram(DTB, 0x8100_0000);
        // copy kernel
        write_ram(IMAGE, 0x8208_0000);
    }

    let mut interpreter = Aarch64Interpreter::new(
        // initial PC is 0x8000_0000
        0x8000_0000,
        TracerKind::Noop,
    );
    interpreter.run();
}

unsafe fn write_ram(data: &[u8], guest_address: usize) {
    // speedy version
    core::ptr::copy(data.as_ptr(), guest_address as *mut u8, data.len());

    // // tracing version
    // for (i, byte) in data.iter().enumerate() {
    //     let byte_address = guest_address + i;
    //     unsafe { *((byte_address) as *mut u8) = *byte };
    //     //  println!("[Sail] mem {byte_address:016x} <- {byte:016x}");
    // }
}
