use {
    crate::{
        arch::x86::memory::PhysAddrExt,
        devices::{Bus, pcie::PCIEBus},
    },
    acpi::{AcpiTables, Handler as AcpiHandler, PhysicalMapping, platform::PciConfigRegions},
    core::ptr::NonNull,
    x86_64::PhysAddr,
};

pub struct ACPIBus;

impl Bus<PhysAddr> for ACPIBus {
    fn probe(&self, probe_data: PhysAddr) {
        let tables =
            unsafe { AcpiTables::from_rsdp(Handler, probe_data.as_u64() as usize) }.unwrap();

        log::error!("pcie probe");
        PCIEBus.probe(PciConfigRegions::new(&tables).unwrap())
    }
}

#[derive(Clone)]
struct Handler;

impl AcpiHandler for Handler {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> acpi::PhysicalMapping<Self, T> {
        let virt_addr = PhysAddr::new(u64::try_from(physical_address).unwrap()).to_virt();

        // log::error!("acpi map {:x} -> {:?} ({:#x})", physical_address, virt_addr,
        // size);

        PhysicalMapping {
            physical_start: physical_address,
            virtual_start: NonNull::new(virt_addr.as_mut_ptr()).unwrap(),
            region_length: size,
            mapped_length: size,
            handler: Self,
        }
    }

    fn unmap_physical_region<T>(_region: &acpi::PhysicalMapping<Self, T>) {}

    fn read_u8(&self, address: usize) -> u8 {
        todo!()
    }

    fn read_u16(&self, address: usize) -> u16 {
        todo!()
    }

    fn read_u32(&self, address: usize) -> u32 {
        todo!()
    }

    fn read_u64(&self, address: usize) -> u64 {
        todo!()
    }

    fn write_u8(&self, address: usize, value: u8) {
        todo!()
    }

    fn write_u16(&self, address: usize, value: u16) {
        todo!()
    }

    fn write_u32(&self, address: usize, value: u32) {
        todo!()
    }

    fn write_u64(&self, address: usize, value: u64) {
        todo!()
    }

    fn read_io_u8(&self, port: u16) -> u8 {
        todo!()
    }

    fn read_io_u16(&self, port: u16) -> u16 {
        todo!()
    }

    fn read_io_u32(&self, port: u16) -> u32 {
        todo!()
    }

    fn write_io_u8(&self, port: u16, value: u8) {
        todo!()
    }

    fn write_io_u16(&self, port: u16, value: u16) {
        todo!()
    }

    fn write_io_u32(&self, port: u16, value: u32) {
        todo!()
    }

    fn read_pci_u8(&self, address: acpi::PciAddress, offset: u16) -> u8 {
        todo!()
    }

    fn read_pci_u16(&self, address: acpi::PciAddress, offset: u16) -> u16 {
        todo!()
    }

    fn read_pci_u32(&self, address: acpi::PciAddress, offset: u16) -> u32 {
        todo!()
    }

    fn write_pci_u8(&self, address: acpi::PciAddress, offset: u16, value: u8) {
        todo!()
    }

    fn write_pci_u16(&self, address: acpi::PciAddress, offset: u16, value: u16) {
        todo!()
    }

    fn write_pci_u32(&self, address: acpi::PciAddress, offset: u16, value: u32) {
        todo!()
    }

    fn nanos_since_boot(&self) -> u64 {
        todo!()
    }

    fn stall(&self, microseconds: u64) {
        todo!()
    }

    fn sleep(&self, milliseconds: u64) {
        todo!()
    }

    fn create_mutex(&self) -> acpi::Handle {
        todo!()
    }

    fn acquire(&self, mutex: acpi::Handle, timeout: u16) -> Result<(), acpi::aml::AmlError> {
        todo!()
    }

    fn release(&self, mutex: acpi::Handle) {
        todo!()
    }
}
