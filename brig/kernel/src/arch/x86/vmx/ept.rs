use {
    crate::arch::x86::{
        memory::{PhysAddrExt, VirtAddrExt},
        vmx::allocate_page,
    },
    alloc::{alloc::alloc_zeroed, boxed::Box},
    bitfields::bitfield,
    common::bits::bit_extract,
    core::{alloc::Layout, array, ptr::null_mut},
    x86_64::{PhysAddr, VirtAddr},
};

//

#[bitfield(u64)]
struct EptEntry {
    /// Read access; indicates whether reads are allowed from the 512-GByte
    /// region controlled by this entry
    read: bool,
    write: bool,
    execute: bool,

    #[bits(5)]
    reserved: u16,

    #[bits(4)]
    ignored0: u8,

    #[bits(36)]
    address: u64,

    #[bits(16)]
    ignored1: u64,
}

#[repr(C, align(4096))]
struct EptTable {
    entries: [EptEntry; 0x200],
}

impl EptTable {
    pub fn from_phys_addr(phys: PhysAddr) -> &'static mut EptTable {
        unsafe { &mut *phys.to_virt().as_mut_ptr::<EptTable>() }
    }
}

pub struct Ept {
    pml4: PhysAddr,
    next_table: PhysAddr,
    end_table: PhysAddr,
}

impl Ept {
    pub fn new() -> Self {
        let num_tables = 1024;
        // 1024 tables
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

    pub fn map_page(&mut self, guest_phys_addr: u64, host_phys_addr: u64) {
        let PageTableIndices { pml4, pdp, pd, pt } =
            PageTableIndices::from_address(guest_phys_addr);

        // log::error!(
        //     "ept map {guest_phys_addr:x} -> {host_phys_addr:x} {:x} {:x}",
        //     self.next_table,
        //     self.end_table
        // );

        let pml4_entry = &mut EptTable::from_phys_addr(self.pml4).entries[usize::from(pml4)];

        if pml4_entry.address() == 0 {
            pml4_entry.set_address(self.allocate_page_table().as_u64() >> 12);
            pml4_entry.set_read(true);
            pml4_entry.set_write(true);
            pml4_entry.set_execute(true);
        }

        let pdp_entry = &mut EptTable::from_phys_addr(PhysAddr::new(pml4_entry.address() << 12))
            .entries[usize::from(pdp)];

        if pdp_entry.address() == 0 {
            pdp_entry.set_address(self.allocate_page_table().as_u64() >> 12);
            pdp_entry.set_read(true);
            pdp_entry.set_write(true);
            pdp_entry.set_execute(true);
        }

        let pd_entry = &mut EptTable::from_phys_addr(PhysAddr::new(pdp_entry.address() << 12))
            .entries[usize::from(pd)];

        if pd_entry.address() == 0 {
            pd_entry.set_address(self.allocate_page_table().as_u64() >> 12);
            pd_entry.set_read(true);
            pd_entry.set_write(true);
            pd_entry.set_execute(true);
        }

        let pt_entry = &mut EptTable::from_phys_addr(PhysAddr::new(pd_entry.address() << 12))
            .entries[usize::from(pt)];

        pt_entry.set_address(host_phys_addr >> 12);
        pt_entry.set_read(true);
        pt_entry.set_write(true);
        pt_entry.set_execute(true);
    }

    pub fn invalidate(&self) {
        // todo
    }

    pub fn phys_addr(&self) -> PhysAddr {
        self.pml4
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
