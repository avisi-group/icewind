use {
    super::Bus,
    crate::host::{
        arch::x86::memory::PhysAddrExt,
        devices::{ivshmem::probe_ivshmem, virtio::probe_virtio_block},
    },
    acpi::{PciConfigRegions, mcfg::PciConfigEntry},
    common::hashmap::HashMap,
    core::fmt::{self, Display},
    log::trace,
    virtio_drivers::transport::pci::bus::{
        Cam, DeviceFunction, DeviceFunctionInfo, MmioCam, PciRoot,
    },
    x86_64::{self, PhysAddr},
};

pub mod bar;

pub struct PCIEBus;

impl Bus<PciConfigRegions<'_, alloc::alloc::Global>> for PCIEBus {
    fn probe(&self, probe_data: PciConfigRegions<'_, alloc::alloc::Global>) {
        probe_data.iter().for_each(enumerate);
    }
}

type ProbeFn = fn(&mut PciRoot<MmioCam>, DeviceFunction);

pub fn enumerate(
    PciConfigEntry {
        bus_range,
        physical_address,
        ..
    }: PciConfigEntry,
) {
    // todo: load me from plugins?
    let pci_driver_map = [
        (PciId::new(0x1af4, 0x1001), probe_virtio_block as ProbeFn),
        (PciId::new(0x8086, 0x2922), probe_ich9r),
        (PciId::new(0x1af4, 0x1110), probe_ivshmem),
    ]
    .into_iter()
    .collect::<HashMap<_, _>>();

    let physical_address = PhysAddr::new(u64::try_from(physical_address).unwrap());
    log::debug!("enumerating pcie {:?} {:x}", bus_range, physical_address);

    let mut root =
        PciRoot::new(unsafe { MmioCam::new(physical_address.to_virt().as_mut_ptr(), Cam::Ecam) });

    for bus in bus_range {
        root.enumerate_bus(bus).for_each(|(dev_fn, dev_fn_info)| {
            let id = PciId::from(dev_fn_info);

            if let Some(prober) = pci_driver_map.get(&id) {
                prober(&mut root, dev_fn);
            } else {
                log::warn!("unsupported pcie device {}", id);
            }
        });
    }
}

fn probe_ich9r(_root: &mut PciRoot<MmioCam>, _device_function: DeviceFunction) {
    trace!("probing sata controller (todo)");
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct PciId {
    vendor_id: u16,
    device_id: u16,
}

impl PciId {
    pub const fn new(vendor_id: u16, device_id: u16) -> Self {
        Self {
            vendor_id,
            device_id,
        }
    }

    pub fn vendor_id(&self) -> u16 {
        self.vendor_id
    }

    pub fn device_id(&self) -> u16 {
        self.device_id
    }
}

impl From<DeviceFunctionInfo> for PciId {
    fn from(info: DeviceFunctionInfo) -> Self {
        Self::new(info.vendor_id, info.device_id)
    }
}

impl Display for PciId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}:{:04x}", self.vendor_id(), self.device_id())
    }
}
