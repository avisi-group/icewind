use {
    crate::host::arch::x86::memory::{PhysAddrExt, VirtualMemoryArea},
    virtio_drivers::transport::pci::bus::{
        BarInfo, DeviceFunction, MemoryBarType, MmioCam, PciRoot,
    },
    x86_64::{
        PhysAddr,
        structures::paging::{Page, PageSize, PageTableFlags, PhysFrame, Size4KiB},
    },
};

/// Map all 64-bit memory BARs of a device to virtual memory
pub fn allocate_bars(root: &mut PciRoot<MmioCam>, device_function: DeviceFunction) {
    root.bars(device_function)
        .unwrap()
        .into_iter()
        // remove None's
        .flatten()
        // remove IO and 32-bit memory BARs
        .filter_map(|bar| match bar {
            BarInfo::Memory {
                address_type: MemoryBarType::Width64,
                address,
                size,
                ..
            } => Some((address, size)),
            _ => None,
        })
        // for each mappable 64-bit BAR...
        .for_each(|(address, size)| {
            // ...calculate it's size in pages...
            let num_pages = u64::from(size) / Size4KiB::SIZE;

            (0..num_pages)
                // ...calculate the offset of each page...
                .map(|page_idx| page_idx * Size4KiB::SIZE)
                // ...calculate the physical and virtual addresses of each page...
                .map(|page_offset| {
                    let phys = PhysAddr::new(address + page_offset);
                    let virt = phys.to_virt();
                    (phys, virt)
                })
                // ...and map them
                .for_each(|(phys, virt)| {
                    VirtualMemoryArea::current().map_page(
                        Page::<Size4KiB>::from_start_address(virt).unwrap(),
                        PhysFrame::<Size4KiB>::from_start_address(phys).unwrap(),
                        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                    );
                });
        });

    VirtualMemoryArea::current().invalidate();
}
