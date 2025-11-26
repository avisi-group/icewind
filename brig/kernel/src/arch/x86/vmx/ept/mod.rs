use {
    crate::arch::x86::{
        memory::{PhysAddrExt, VirtAddrExt},
        vmx::ept::invalidation::invept_all_contexts,
    },
    alloc::alloc::alloc_zeroed,
    bitfields::bitfield,
    common::bits::bit_extract,
    core::{alloc::Layout, ops::Range},
    spin::{Lazy, Mutex},
    x86_64::{PhysAddr, VirtAddr},
};

pub mod invalidation;

pub static EPT: Lazy<Mutex<Ept>> = Lazy::new(|| Mutex::new(Ept::new()));

pub fn init() {
    let mut ept = EPT.lock();
    for i in 0..16 {
        ept.map_1g_page(0x4000_0000 * i, 0x4000_0000 * i);
    }
}

trait Entry {}

impl Entry for EptEntry {}
impl Entry for Pdp1GEntry {}

#[bitfield(u64)]
pub struct EptEntry {
    /// Read access; indicates whether reads are allowed from the 512-GByte
    /// region controlled by this entry
    read: bool,
    write: bool,
    execute: bool,

    #[bits(4)]
    reserved: u16,

    size: bool,

    #[bits(4)]
    ignored0: u8,

    #[bits(36)]
    address: u64,

    #[bits(16)]
    ignored1: u64,
}

#[bitfield(u64)]
pub struct Pdp1GEntry {
    /// Read access; indicates whether reads are allowed from the 512-GByte
    /// region controlled by this entry
    read: bool,
    write: bool,
    execute: bool,

    #[bits(3)]
    ept_memory_type: u8,

    ignore_pat: bool,
    size: bool,
    accessed: bool,
    dirty: bool,
    user_execute_access: bool,
    ignored0: bool,

    #[bits(40)]
    address: u64,

    #[bits(12)]
    ignored1: u64,
}

#[repr(C, align(4096))]
struct EptTable<E> {
    entries: [E; 0x200],
}

impl<E: Entry> EptTable<E> {
    pub fn from_phys_addr(phys: PhysAddr) -> &'static mut EptTable<E> {
        unsafe { &mut *phys.to_virt().as_mut_ptr::<EptTable<E>>() }
    }
}

pub struct Ept {
    pml4: PhysAddr,
    next_table: PhysAddr,
    end_table: PhysAddr,
}

impl Ept {
    fn new() -> Self {
        let num_tables = 4096;

        let tables = VirtAddr::from_ptr(unsafe {
            alloc_zeroed(Layout::from_size_align(num_tables * 4096, 4096).unwrap())
        })
        .to_phys();

        let mut selph = Self {
            pml4: PhysAddr::zero(),
            next_table: tables,
            end_table: tables + u64::try_from(num_tables * 4096).unwrap(),
        };

        selph.pml4 = selph.allocate_page_table();

        selph
    }

    fn allocate_page_table(&mut self) -> PhysAddr {
        if self.next_table >= self.end_table {
            panic!("ept out of tables");
        }

        let table = self.next_table;
        self.next_table += 4096;

        table
    }

    pub fn map_1g_page(&mut self, guest_phys_addr: u64, host_phys_addr: u64) {
        let PageTableIndices { pml4, pdp, pd, pt } =
            PageTableIndices::from_address(guest_phys_addr);

        assert!(guest_phys_addr & 0x3FFF_FFFF == 0);
        assert!(host_phys_addr & 0x3FFF_FFFF == 0);

        let pml4_entry =
            &mut EptTable::<EptEntry>::from_phys_addr(self.pml4).entries[usize::from(pml4)];

        if pml4_entry.0 == 0 {
            pml4_entry.set_address(self.allocate_page_table().as_u64() >> 12);
            pml4_entry.set_read(true);
            pml4_entry.set_write(true);
            pml4_entry.set_execute(true);
        }

        let pdp_entry =
            &mut EptTable::<Pdp1GEntry>::from_phys_addr(PhysAddr::new(pml4_entry.address() << 12))
                .entries[usize::from(pdp)];

        pdp_entry.set_address(host_phys_addr >> 12);
        pdp_entry.set_read(true);
        pdp_entry.set_write(true);
        pdp_entry.set_execute(true);
        pdp_entry.set_size(true);
    }

    pub fn invalidate(&self) {
        // let masked_addr = self.phys_addr().as_u64() & !0xFFF;
        // let memory_type = 0; // writeback (0 = uncacheable)
        // This value is 1 less than the EPT page-walk length
        // let page_walk_length = 3 << 3;
        // let eptp = masked_addr | memory_type | page_walk_length;
        // invept_single_context(eptp);
        invept_all_contexts();
    }

    pub fn phys_addr(&self) -> PhysAddr {
        self.pml4
    }

    /// Updates the flags of the pages mapped to virtual addresses in the
    /// supplied range
    pub fn update_flags_range(&mut self, range: Range<u64>, flags: Flags) {
        range.step_by(4096).for_each(|addr| {
            let entry = self.translate(addr);
            entry.set_read(flags.read);
            entry.set_write(flags.write);
            entry.set_execute(flags.execute);
        });
    }

    pub fn smc_protect(&mut self, page: u64) {
        let entry = self.translate(page);
        entry.set_write(false);

        unsafe {
            core::arch::asm!("vmfunc", in("rax") 0, in("rcx") 0);
        };
    }

    pub fn translate<'a>(&'a mut self, guest_phys_addr: u64) -> &'a mut EptEntry {
        let PageTableIndices { pml4, pdp, pd, pt } =
            PageTableIndices::from_address(guest_phys_addr);

        // log::error!(
        //     "ept map {guest_phys_addr:x} -> {host_phys_addr:x} {:x} {:x}",
        //     self.next_table,
        //     self.end_table
        // );

        let pml4_entry =
            &mut EptTable::<EptEntry>::from_phys_addr(self.pml4).entries[usize::from(pml4)];

        if pml4_entry.0 == 0 {
            pml4_entry.set_address(self.allocate_page_table().as_u64() >> 12);
            pml4_entry.set_read(true);
            pml4_entry.set_write(true);
            pml4_entry.set_execute(true);
        }

        let pdp_entry =
            &mut EptTable::<EptEntry>::from_phys_addr(PhysAddr::new(pml4_entry.address() << 12))
                .entries[usize::from(pdp)];

        if pdp_entry.0 == 0 {
            pdp_entry.set_address(self.allocate_page_table().as_u64() >> 12);
            pdp_entry.set_read(true);
            pdp_entry.set_write(true);
            pdp_entry.set_execute(true);
        } else {
            if pdp_entry.size() {
                panic!("attempting to map through a 1g page");
            }
        }

        let pd_entry =
            &mut EptTable::<EptEntry>::from_phys_addr(PhysAddr::new(pdp_entry.address() << 12))
                .entries[usize::from(pd)];

        if pd_entry.0 == 0 {
            pd_entry.set_address(self.allocate_page_table().as_u64() >> 12);
            pd_entry.set_read(true);
            pd_entry.set_write(true);
            pd_entry.set_execute(true);
        }

        &mut EptTable::from_phys_addr(PhysAddr::new(pd_entry.address() << 12)).entries
            [usize::from(pt)]
    }
}

struct PageTableIndices {
    pml4: u16,
    pdp: u16,
    pd: u16,
    pt: u16,
}

impl PageTableIndices {
    fn from_address(address: u64) -> Self {
        Self {
            pml4: u16::try_from(bit_extract(address, 39, 9)).unwrap(),
            pdp: u16::try_from(bit_extract(address, 30, 9)).unwrap(),
            pd: u16::try_from(bit_extract(address, 21, 9)).unwrap(),
            pt: u16::try_from(bit_extract(address, 12, 9)).unwrap(),
        }
    }
}

pub struct Flags {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}
