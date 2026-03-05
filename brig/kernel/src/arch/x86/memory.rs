use {
    alloc::{
        alloc::{Global, alloc_zeroed},
        boxed::Box,
        vec::Vec,
    },
    bootloader_api::info::{MemoryRegionKind, MemoryRegions},
    buddy_system_allocator::LockedHeap,
    common::bytes,
    core::{
        alloc::{AllocError, Allocator, Layout},
        ops::{Deref, Range},
        ptr::NonNull,
    },
    spin::{Lazy, Mutex},
    x86_64::{
        PhysAddr, VirtAddr,
        registers::control::{Cr3, Cr3Flags},
        structures::paging::{
            FrameAllocator, Mapper, OffsetPageTable, Page, PageSize, PageTable, PageTableFlags,
            PhysFrame, Size4KiB, Translate,
            mapper::{MappedFrame, TranslateResult},
            page_table::PageTableLevel,
        },
    },
};

/// 32 GiB
pub const GUEST_PHYSICAL_SIZE: u64 = 0x8_0000_0000;

pub const _LOW_HALF_CANONICAL_START: VirtAddr = VirtAddr::new_truncate(0x0000_0000_0000_0000);
pub const LOW_HALF_CANONICAL_END: VirtAddr = VirtAddr::new_truncate(0x0000_7fff_ffff_ffff);
pub const HIGH_HALF_CANONICAL_START: VirtAddr = VirtAddr::new_truncate(0xffff_8000_0000_0000);
pub const HIGH_HALF_CANONICAL_END: VirtAddr = VirtAddr::new_truncate(0xffff_ffff_ffff_ffff);
pub const PHYSICAL_MEMORY_OFFSET: VirtAddr = VirtAddr::new_truncate(0xffff_8180_0000_0000);
pub const GUEST_PHYSICAL_START: VirtAddr = VirtAddr::new_truncate(0xffff_9000_0000_0000);
pub const GUEST_PHYSICAL_END: VirtAddr =
    VirtAddr::new_truncate(0xffff_9000_0000_0000 + GUEST_PHYSICAL_SIZE);

pub const VIRT_GUEST_EMULATED_GUEST_PHYSICAL_START: PhysAddr = PhysAddr::new(0x40_0000_0000); // 0x0 + 256 GiB
pub const VIRT_GUEST_EMULATED_GUEST_PHYSICAL_END: PhysAddr = PhysAddr::new(0x50_0000_0000); // start + 64 GiB

pub fn guest_physical_to_host_virt(guest_physical: u64) -> VirtAddr {
    GUEST_PHYSICAL_START + guest_physical
}

#[global_allocator]
pub static HEAP_ALLOCATOR: LockedHeap<64> = LockedHeap::empty();

/// Guest physical addresses of pages that should be flushed from the code cache
pub static STALE_CODE_PAGES: Lazy<Mutex<Vec<u64>>> = Lazy::new(|| Mutex::new(Vec::new()));

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
        bytes(HEAP_ALLOCATOR.lock().stats_total_bytes())
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

    pub fn invalidate_guest_mappings(&mut self) {
        self.opt
            .level_4_table_mut()
            .iter_mut()
            .take(0x100) //only clear guest half of address space
            .for_each(|e| {
                let mut flags = e.flags();
                flags.remove(PageTableFlags::PRESENT);
                e.set_flags(flags)
            });
        self.invalidate();
    }

    pub fn smc_protect(&mut self) {
        self.opt
            .level_4_table_mut()
            .iter_mut()
            .take(0x100) //only clear guest half of address space
            .for_each(|e| {
                let mut flags = e.flags();
                flags.remove(PageTableFlags::WRITABLE);
                e.set_flags(flags)
            });
        self.invalidate();
    }

    pub fn release_smc_protection(&mut self, address: VirtAddr) -> bool {
        let l3_table =
            walk_table_smc_reset(self.opt.level_4_table_mut(), address, PageTableLevel::Four);
        let l2_table = walk_table_smc_reset(l3_table, address, PageTableLevel::Three);
        let l1_table = walk_table_smc_reset(l2_table, address, PageTableLevel::Two);

        let l1_entry = &mut l1_table[address.p1_index()];

        l1_entry.set_flags(l1_entry.flags().union(PageTableFlags::WRITABLE));

        false
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
        unsafe {
            let _flush = self
                .opt
                .map_to(page, frame, flags, &mut HeapStealingFrameAllocator)
                .unwrap();
        }
    }

    pub fn map_page_propagate_invalidation<S: PageSize + core::fmt::Debug>(
        &mut self,
        page: Page<S>,
        frame: PhysFrame<S>,
        flags: PageTableFlags,
    ) where
        OffsetPageTable<'static>: Mapper<S>,
    {
        log::trace!(
            "map page propagate invalidation: page: {page:x?}, frame: {frame:x?}, flags: {flags:?}"
        );

        let l3_table = walk_and_reset_table(
            self.opt.level_4_table_mut(),
            page.start_address(),
            PageTableLevel::Four,
        );
        let l2_table = walk_and_reset_table(l3_table, page.start_address(), PageTableLevel::Three);
        let l1_table = walk_and_reset_table(l2_table, page.start_address(), PageTableLevel::Two);

        let l1_entry = &mut l1_table[page.start_address().p1_index()];

        l1_entry.set_addr(frame.start_address(), flags);
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

/// Gets the VirtAddr of a Box's pointer
pub trait BoxToVirtAddrExt {
    fn as_virt(&self) -> VirtAddr;
}

impl<T> BoxToVirtAddrExt for Box<T> {
    fn as_virt(&self) -> VirtAddr {
        VirtAddr::from_ptr(&**self as *const T)
    }
}

pub struct AlignedAllocator<const N: usize>;

unsafe impl<const N: usize> Allocator for AlignedAllocator<N> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        Global.allocate(layout.align_to(N).unwrap())
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { Global.deallocate(ptr, layout.align_to(N).unwrap()) }
    }
}

fn clear_all_present_bits(table: &mut PageTable) {
    table.iter_mut().for_each(|e| {
        let mut flags = e.flags();
        flags.remove(PageTableFlags::PRESENT);
        e.set_flags(flags)
    });
}

fn walk_and_reset_table(
    table: &mut PageTable,
    address: VirtAddr,
    level: PageTableLevel,
) -> &mut PageTable {
    let entry = &mut table[address.page_table_index(level)];
    let mut should_clear = false;

    if !entry.flags().contains(PageTableFlags::PRESENT) {
        if entry.is_unused() {
            let frame = HeapStealingFrameAllocator.allocate_frame().unwrap();
            entry.set_addr(
                frame.start_address(),
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            );
        } else {
            entry.set_flags(entry.flags().union(PageTableFlags::PRESENT));
            should_clear = true;
        }
    }

    if entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        panic!();
    }

    let next_table = unsafe { &mut *entry.addr().to_virt().as_mut_ptr::<PageTable>() };

    if should_clear {
        clear_all_present_bits(next_table);
    }

    next_table
}

fn walk_table_smc_reset(
    table: &mut PageTable,
    address: VirtAddr,
    level: PageTableLevel,
) -> &mut PageTable {
    let entry = &mut table[address.page_table_index(level)];
    let mut should_clear = false;

    if !entry.flags().contains(PageTableFlags::WRITABLE) {
        if entry.is_unused() {
            let frame = HeapStealingFrameAllocator.allocate_frame().unwrap();
            entry.set_addr(
                frame.start_address(),
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            );
        } else {
            entry.set_flags(entry.flags().union(PageTableFlags::WRITABLE));
            should_clear = true;
        }
    }

    if entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        panic!();
    }

    let next_table = unsafe { &mut *entry.addr().to_virt().as_mut_ptr::<PageTable>() };

    if should_clear {
        clear_all_writeable_bits(next_table);
    }

    next_table
}

fn clear_all_writeable_bits(table: &mut PageTable) {
    table.iter_mut().for_each(|e| {
        let mut flags = e.flags();
        flags.remove(PageTableFlags::WRITABLE);
        e.set_flags(flags)
    });
}
