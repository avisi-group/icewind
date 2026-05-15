use {
    alloc::alloc::{alloc_zeroed, Global},
    bootloader_api::info::{MemoryRegionKind, MemoryRegions},
    buddy_system_allocator::LockedHeap,
    byte_unit::{Byte, UnitType},
    core::{
        alloc::{AllocError, Allocator, Layout},
        ops::{Deref, Range},
        ptr::NonNull,
    },
    x86_64::{
        registers::control::{Cr3, Cr3Flags},
        structures::paging::{
            mapper::{MappedFrame, TranslateResult},
            FrameAllocator, Mapper, OffsetPageTable, Page, PageSize, PageTableFlags, PhysFrame,
            Size4KiB, Translate,
        },
        PhysAddr, VirtAddr,
    },
};

pub const _LOW_HALF_CANONICAL_START: VirtAddr = VirtAddr::new_truncate(0x0000_0000_0000_0000);
pub const LOW_HALF_CANONICAL_END: VirtAddr = VirtAddr::new_truncate(0x0000_7fff_ffff_ffff);
pub const HIGH_HALF_CANONICAL_START: VirtAddr = VirtAddr::new_truncate(0xffff_8000_0000_0000);
pub const HIGH_HALF_CANONICAL_END: VirtAddr = VirtAddr::new_truncate(0xffff_ffff_ffff_ffff);
pub const PHYSICAL_MEMORY_OFFSET: VirtAddr = VirtAddr::new_truncate(0xffff_8180_0000_0000);

#[global_allocator]
pub static HEAP_ALLOCATOR: LockedHeap<64> = LockedHeap::empty();

/// Initialize the global heap allocator backed by the usable memory regions
/// supplied by the bootloader
pub fn heap_init(memory_regions: &MemoryRegions) {
    // get usable regions from memory map and add to heap allocator
    for region in memory_regions
        .deref()
        .iter()
        .filter(|r| matches!(r.kind, MemoryRegionKind::Usable))
    {
        let region_virt_start = PhysAddr::new(region.start).to_virt();
        let region_virt_end = PhysAddr::new(region.end).to_virt();

        unsafe {
            HEAP_ALLOCATOR.lock().add_to_heap(
                usize::try_from(region_virt_start.as_u64()).unwrap(),
                usize::try_from(region_virt_end.as_u64()).unwrap(),
            )
        };
    }

    log::info!(
        "heap allocator initialized @ {:p}, {:.2} available",
        &HEAP_ALLOCATOR as *const _,
        Byte::from(HEAP_ALLOCATOR.lock().stats_total_bytes())
            .get_appropriate_unit(UnitType::Binary)
    );
}

/// Returns the number of bytes used by and number of bytes available to the
/// heap allocator
pub fn stats() -> (usize, usize) {
    unsafe { HEAP_ALLOCATOR.force_unlock() };
    let allocator = HEAP_ALLOCATOR.lock();
    (
        allocator.stats_alloc_actual(),
        allocator.stats_total_bytes(),
    )
}

/// Frame allocator that uses the global heap allocator, then translates virtual
/// addresses back to physical
struct HeapStealingFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for HeapStealingFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let new_frame = unsafe {
            alloc_zeroed(
                Layout::from_size_align(
                    Size4KiB::SIZE.try_into().unwrap(),
                    Size4KiB::SIZE.try_into().unwrap(),
                )
                .unwrap(),
            )
        };

        Some(PhysFrame::from_start_address(VirtAddr::from_ptr(new_frame).to_phys()).unwrap())
    }
}

pub struct VirtualMemoryArea {
    pub pml4_base: PhysAddr,
    pub opt: OffsetPageTable<'static>,
}

impl VirtualMemoryArea {
    fn get_current_cr3() -> PhysAddr {
        Cr3::read().0.start_address()
    }

    pub fn current() -> Self {
        let pml4_base = Self::get_current_cr3();
        let pml4_virt = pml4_base.to_virt();
        let pml4_table = unsafe { &mut *(pml4_virt.as_mut_ptr()) };

        Self {
            pml4_base,
            opt: unsafe { OffsetPageTable::new(pml4_table, PHYSICAL_MEMORY_OFFSET) },
        }
    }

    pub fn invalidate(&self) {
        assert!(Self::get_current_cr3() == self.pml4_base);
        self.activate();
    }

    pub fn activate(&self) {
        unsafe {
            Cr3::write(
                PhysFrame::from_start_address(self.pml4_base).unwrap(),
                Cr3Flags::empty(),
            );
        }
    }

    pub fn map_page<S: PageSize + core::fmt::Debug>(
        &mut self,
        page: Page<S>,
        frame: PhysFrame<S>,
        flags: PageTableFlags,
    ) where
        OffsetPageTable<'static>: Mapper<S>,
    {
        //       log::error!("");
        unsafe {
            let _ = self
                .opt
                .map_to(page, frame, flags, &mut HeapStealingFrameAllocator)
                .unwrap();
        }
    }

    pub fn translate_address(&self, addr: VirtAddr) -> Option<PhysAddr> {
        let r = self.opt.translate_addr(addr);

        //log::trace!("translating {:x} to {:?}", addr, r);

        r
    }

    /// Updates the flags of the pages mapped to virtual addresses in the
    /// supplied range
    pub fn update_flags_range(&mut self, range: Range<VirtAddr>, flags: PageTableFlags) {
        /// Update the flags of the page at the supplied physical frame address,
        /// returning the size of that physical frame
        fn update_flags<S: PageSize>(
            page_table: &mut OffsetPageTable,
            phys: PhysFrame<S>,
            flags: PageTableFlags,
        ) -> u64
        where
            for<'a> OffsetPageTable<'a>: Mapper<S>,
        {
            let page = Page::<S>::from_start_address(phys.start_address().to_virt()).unwrap();

            unsafe { page_table.update_flags(page, flags) }
                .unwrap()
                .flush();

            S::SIZE
        }

        let mut current_virt_addr = range.start.as_u64();

        while current_virt_addr < range.end.as_u64() {
            let virt_frame = VirtAddr::new(current_virt_addr);

            let TranslateResult::Mapped { frame, .. } = self.opt.translate(virt_frame) else {
                panic!("region not mapped");
            };

            current_virt_addr += match frame {
                MappedFrame::Size4KiB(phys) => update_flags(&mut self.opt, phys, flags),
                MappedFrame::Size2MiB(phys) => update_flags(&mut self.opt, phys, flags),
                MappedFrame::Size1GiB(phys) => update_flags(&mut self.opt, phys, flags),
            };
        }
    }
}

/// Address extension trait for additional methods on `PhysAddr`
pub trait PhysAddrExt {
    fn to_virt(&self) -> VirtAddr;
}

impl PhysAddrExt for PhysAddr {
    fn to_virt(&self) -> VirtAddr {
        VirtAddr::new(self.as_u64() + PHYSICAL_MEMORY_OFFSET.as_u64())
    }
}

/// Address extension trait for additional methods on `VirtAddr`
pub trait VirtAddrExt {
    fn to_phys(&self) -> PhysAddr;
}

impl VirtAddrExt for VirtAddr {
    fn to_phys(&self) -> PhysAddr {
        PhysAddr::new(self.as_u64() - PHYSICAL_MEMORY_OFFSET.as_u64())
    }
}

pub struct AlignedAllocator<const N: usize>;

unsafe impl<const N: usize> Allocator for AlignedAllocator<N> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        Global.allocate(layout.align_to(N).unwrap())
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        Global.deallocate(ptr, layout.align_to(N).unwrap())
    }
}
