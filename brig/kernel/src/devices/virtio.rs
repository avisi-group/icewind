use {
    crate::{
        arch::x86::memory::{PhysAddrExt, VirtAddrExt, VirtualMemoryArea},
        devices::{
            BlockDevice, Device, SharedDevice, manager::SharedDeviceManager,
            pcie::bar::allocate_bars,
        },
    },
    alloc::{
        alloc::{alloc_zeroed, dealloc},
        boxed::Box,
        format,
    },
    common::bytes,
    core::{alloc::Layout, fmt::Debug, ptr::NonNull},
    log::trace,
    spin::Mutex,
    virtio_drivers::{
        device::blk::{SECTOR_SIZE, VirtIOBlk},
        transport::pci::{
            PciTransport,
            bus::{Command, DeviceFunction, MmioCam, PciRoot},
        },
    },
    x86_64::{PhysAddr, VirtAddr},
};

struct VirtioHal;

unsafe impl virtio_drivers::Hal for VirtioHal {
    fn dma_alloc(
        pages: usize,
        _direction: virtio_drivers::BufferDirection,
    ) -> (virtio_drivers::PhysAddr, NonNull<u8>) {
        let ptr = unsafe {
            alloc_zeroed(
                Layout::from_size_align(
                    pages * virtio_drivers::PAGE_SIZE,
                    virtio_drivers::PAGE_SIZE,
                )
                .unwrap(),
            )
        };

        let paddr = VirtAddr::from_ptr(ptr)
            .to_phys()
            .as_u64()
            .try_into()
            .unwrap();

        let vaddr = NonNull::new(ptr).unwrap();

        (paddr, vaddr)
    }

    unsafe fn dma_dealloc(
        paddr: virtio_drivers::PhysAddr,
        vaddr: NonNull<u8>,
        pages: usize,
    ) -> i32 {
        log::trace!("dma_dealloc: {paddr:x} {vaddr:p} {pages:x}");
        unsafe {
            dealloc(
                vaddr.as_ptr(),
                Layout::from_size_align(
                    pages * virtio_drivers::PAGE_SIZE,
                    virtio_drivers::PAGE_SIZE,
                )
                .unwrap(),
            )
        };

        0
    }

    unsafe fn mmio_phys_to_virt(paddr: virtio_drivers::PhysAddr, _size: usize) -> NonNull<u8> {
        let physical_address = PhysAddr::new(u64::try_from(paddr).unwrap());
        NonNull::new(physical_address.to_virt().as_mut_ptr()).unwrap()
    }

    unsafe fn share(
        buffer: NonNull<[u8]>,
        _direction: virtio_drivers::BufferDirection,
    ) -> virtio_drivers::PhysAddr {
        VirtualMemoryArea::current()
            .translate_address(VirtAddr::from_ptr(buffer.as_ptr() as *const u8))
            .unwrap()
            .as_u64()
            .try_into()
            .unwrap()
    }

    unsafe fn unshare(
        _paddr: virtio_drivers::PhysAddr,
        _buffer: NonNull<[u8]>,
        _direction: virtio_drivers::BufferDirection,
    ) {
    }
}

pub fn probe_virtio_block(root: &mut PciRoot<MmioCam>, device_function: DeviceFunction) {
    trace!("probing virtio block device");

    root.set_command(
        device_function,
        Command::IO_SPACE | Command::MEMORY_SPACE | Command::BUS_MASTER,
    );

    allocate_bars(root, device_function);

    let transport = PciTransport::new::<VirtioHal, _>(root, device_function).unwrap();

    let blk = Mutex::new(VirtIOBlk::<VirtioHal, _>::new(transport).unwrap());

    let dev_mgr = SharedDeviceManager::get();

    let id = dev_mgr.register_device(SharedDevice::from_device(Device::Block(Box::new(
        VirtioBlockDevice {
            blk,
            device_function,
        },
    ))));

    dev_mgr.add_alias(id, format!("disk{}", device_function));
}

struct VirtioBlockDevice {
    blk: Mutex<VirtIOBlk<VirtioHal, PciTransport>>,
    device_function: DeviceFunction,
}

// safe as blk is accessed through mutex
unsafe impl Sync for VirtioBlockDevice {}
unsafe impl Send for VirtioBlockDevice {}

impl Debug for VirtioBlockDevice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "VirtioBlockDevice @ {}, capacity: {:.2}, block size: {:.2}",
            self.device_function,
            bytes(self.size()),
            bytes(self.block_size())
        )
    }
}

impl BlockDevice for VirtioBlockDevice {
    fn block_size(&self) -> usize {
        SECTOR_SIZE
    }

    fn size(&self) -> usize {
        usize::try_from(self.blk.lock().capacity()).unwrap() * self.block_size()
    }

    fn read(&mut self, buf: &mut [u8], start_block_index: usize) -> Result<(), super::IoError> {
        self.blk
            .lock()
            .read_blocks(start_block_index, buf)
            .map_err(|e| panic!("{e:?}"))
    }

    fn write(&mut self, buf: &[u8], start_block_index: usize) -> Result<(), super::IoError> {
        self.blk
            .lock()
            .write_blocks(start_block_index, buf)
            .map_err(|e| panic!("{e:?}"))
    }
}
