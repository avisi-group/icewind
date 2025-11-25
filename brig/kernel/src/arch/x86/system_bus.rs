use {
    crate::devices::{Bus, acpi, lapic},
    x86_64::PhysAddr,
};

pub fn probe(rsdp_addr: u64) {
    log::debug!("acpi probe");
    acpi::ACPIBus.probe(PhysAddr::new(rsdp_addr));

    log::debug!("lapic init");
    lapic::init();
}
