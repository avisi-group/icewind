use {
    crate::arch::x86::{
        memory::{PhysAddrExt, VirtAddrExt},
        vmx::allocate_page,
    },
    alloc::boxed::Box,
    bitfields::bitfield,
    common::bits::bit_extract,
    core::array,
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
    pml4: Box<EptTable>,
}

impl Ept {
    pub fn new() -> Self {
        Self {
            pml4: Box::new(EptTable {
                entries: array::from_fn(|_| EptEntry::from_bits(0)),
            }),
        }
    }

    pub fn map_page(&mut self, guest_phys_addr: u64, host_phys_addr: u64) {
        let PageTableIndices { pml4, pdp, pd, pt } =
            PageTableIndices::from_address(guest_phys_addr);

        let pml4_entry = &mut self.pml4.entries[usize::from(pml4)];

        if pml4_entry.address() == 0 {
            pml4_entry.set_address(allocate_page().to_phys().as_u64() >> 12);
            pml4_entry.set_read(true);
            pml4_entry.set_write(true);
            pml4_entry.set_execute(true);
        }

        let pdp_entry = &mut EptTable::from_phys_addr(PhysAddr::new(pml4_entry.address() << 12))
            .entries[usize::from(pdp)];

        if pdp_entry.address() == 0 {
            pdp_entry.set_address(allocate_page().to_phys().as_u64() >> 12);
            pdp_entry.set_read(true);
            pdp_entry.set_write(true);
            pdp_entry.set_execute(true);
        }

        let pd_entry = &mut EptTable::from_phys_addr(PhysAddr::new(pml4_entry.address() << 12))
            .entries[usize::from(pd)];

        if pd_entry.address() == 0 {
            pd_entry.set_address(allocate_page().to_phys().as_u64() >> 12);
            pd_entry.set_read(true);
            pd_entry.set_write(true);
            pd_entry.set_execute(true);
        }

        let pt_entry = &mut EptTable::from_phys_addr(PhysAddr::new(pml4_entry.address() << 12))
            .entries[usize::from(pt)];

        pt_entry.set_address(host_phys_addr >> 12);
        pt_entry.set_read(true);
        pt_entry.set_write(true);
        pt_entry.set_execute(true);
    }

    pub fn phys_addr(&self) -> PhysAddr {
        VirtAddr::from_ptr((&*self.pml4) as *const EptTable).to_phys()
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
            pml4: u16::try_from(bit_extract(address, 39, 47)).unwrap(),
            pdp: u16::try_from(bit_extract(address, 30, 38)).unwrap(),
            pd: u16::try_from(bit_extract(address, 21, 29)).unwrap(),
            pt: u16::try_from(bit_extract(address, 12, 20)).unwrap(),
        }
    }
}
