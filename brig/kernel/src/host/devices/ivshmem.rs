use {
    crate::host::{
        arch::x86::memory::PhysAddrExt,
        devices::{
            Device, MemDevice, SharedDevice, TransportDevice, manager::SharedDeviceManager,
            pcie::bar::allocate_bars,
        },
    },
    alloc::{boxed::Box, format},
    common::{
        bytes,
        ringbuffer::{Producer, RingBuffer},
    },
    core::fmt::{self, Debug},
    log::trace,
    virtio_drivers::transport::pci::bus::{
        BarInfo, Command, DeviceFunction, MemoryBarType, MmioCam, PciRoot,
    },
    x86_64::PhysAddr,
};

pub fn probe_ivshmem(root: &mut PciRoot<MmioCam>, device_function: DeviceFunction) {
    trace!("probing ishmem");

    root.set_command(
        device_function,
        Command::IO_SPACE | Command::MEMORY_SPACE | Command::BUS_MASTER,
    );

    //     The ivshmem PCI device has two or three BARs:

    // BAR0 holds device registers (256 Byte MMIO)
    // BAR1 holds MSI-X table and PBA (only ivshmem-doorbell)
    // BAR2 maps the shared memory object

    allocate_bars(root, device_function);

    let info = root.bar_info(device_function, 2).unwrap();

    let Some(BarInfo::Memory {
        address_type: MemoryBarType::Width64,
        address: phys_addr,
        size,
        ..
    }) = info
    else {
        panic!()
    };

    let virt = PhysAddr::new(phys_addr).to_virt();

    let mem = unsafe {
        core::slice::from_raw_parts_mut::<u8>(virt.as_mut_ptr(), usize::try_from(size).unwrap())
    };

    let rb = RingBuffer::<Producer>::open(mem);

    let dev_mgr = SharedDeviceManager::get();

    let id = dev_mgr.register_device(SharedDevice::from_device(Device::Transport(Box::new(rb))));
    dev_mgr.add_alias(id, format!("transport{}", device_function));
}

pub struct InterVMSharedMemory(pub &'static mut [u8]);

impl Debug for InterVMSharedMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "InterVMSharedMemory @ {:#x}, size: {}",
            self.0.as_ptr() as usize,
            bytes(self.0.len())
        )
    }
}

impl MemDevice for InterVMSharedMemory {}

impl<'a> TransportDevice for RingBuffer<'a, Producer> {
    fn read(&mut self, _buf: &mut [u8]) -> usize {
        0
    }

    fn write(&mut self, buf: &[u8]) -> usize {
        self.write(buf)
    }
}
