use {
    crate::arch::x86::memory::VirtAddrExt,
    alloc::boxed::Box,
    x86::{
        bits64::vmx::vmxon,
        msr::{IA32_VMX_BASIC, rdmsr},
    },
    x86_64::{PhysAddr, VirtAddr},
};

pub fn init() {
    let vmxon_region = Vmxon::new();

    unsafe { vmxon(vmxon_region.as_phys().as_u64()).unwrap() };
}

/// A representation of the VMXON region in memory.
///
/// The VMXON region is essential for enabling VMX operations on the CPU.
/// This structure offers methods for setting up the VMXON region, enabling VMX
/// operations, and performing related tasks.
///
/// Reference: Intel® 64 and IA-32 Architectures Software Developer's Manual:
/// 25.11.5 VMXON Region
#[repr(C, align(4096))]
pub struct Vmxon {
    /// Revision ID required for VMXON.
    pub revision_id: u32,

    /// Data array constituting the rest of the VMXON region.
    pub data: [u8; 4096 - 4],
}

impl Vmxon {
    /// Initializes the VMXON region.
    pub fn new() -> Box<Self> {
        let revision_id = (unsafe { rdmsr(IA32_VMX_BASIC) } as u32) &
        // clear bit 31
        !(1 << 31);

        Box::new(Self {
            revision_id,
            data: [0u8; _],
        })
    }

    pub fn as_phys(self: &Box<Self>) -> PhysAddr {
        VirtAddr::from_ptr((&**self) as *const Self).to_phys()
    }
}
