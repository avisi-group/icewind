use {
    crate::{
        guest::{
            devices::{arm::a9gic::GlobalInterruptController, primecell::pl011::Pl011},
            memory::{AddressSpace, AddressSpaceRegion},
        },
        host::{
            dbt::{
                models::{self, ModelDevice},
                sysreg_helpers::{self, encode_sysreg_id},
            },
            fs::Filesystem,
            objects::{
                Object, ObjectId, ObjectStore,
                device::{Device, MemoryMappedDevice},
            },
        },
    },
    alloc::{boxed::Box, collections::BTreeMap, sync::Arc},
    common::{TestConfig, intern::InternedString},
    core::{panic, ptr, sync::atomic::AtomicU64},
    elfloader::{ElfBinary, ElfLoader, ElfLoaderErr, ProgramHeader, RelocationEntry},
    spin::Once,
    x86::current::segmentation::{rdfsbase, wrfsbase},
};

pub mod config;
pub mod devices;
pub mod memory;

pub static mut GUEST: Once<Guest> = Once::INIT;

#[derive(Default)]
pub struct Guest {
    pub address_spaces: BTreeMap<InternedString, Box<AddressSpace>>,
    pub devices: BTreeMap<InternedString, Arc<dyn Device>>,
}

impl Guest {
    pub fn new() -> Self {
        Self::default()
    }
}

#[repr(C)]
pub struct GuestExecutionContext {
    pub current_address_space: *mut AddressSpace,
    pub interrupt_pending: AtomicU64,
    pub unprivileged_access: u64,
}

impl GuestExecutionContext {
    pub fn activate(self: Box<Self>) {
        unsafe {
            wrfsbase(Box::into_raw(self) as u64);
        }
    }

    pub fn current() -> &'static Self {
        unsafe { &*(rdfsbase() as *const Self) }
    }

    pub fn current_mut() -> &'static mut Self {
        unsafe { &mut *(rdfsbase() as *mut Self) }
    }
}

/// Start guest emulation
pub fn start<FS: Filesystem>(guest_data: &mut FS, test_config: TestConfig) {
    unsafe { GUEST.call_once(Guest::new) };
    let guest = unsafe { GUEST.get_mut() }.unwrap();

    // create memory
    let mut addrspace = AddressSpace::new();
    addrspace.add_region(AddressSpaceRegion::new(
        "ram0".into(),
        0x4000_0000,
        512 * 1024 * 1024,
        memory::AddressSpaceRegionKind::Ram,
    ));
    addrspace.add_region(AddressSpaceRegion::new(
        "ram1".into(),
        0x8000_0000,
        1 * 1024 * 1024 * 1024,
        memory::AddressSpaceRegionKind::Ram,
    ));
    guest
        .address_spaces
        .insert("as0".into(), Box::new(addrspace));

    // core
    let model = models::get("aarch64").unwrap();
    let initial_pc = 0x4000_06b0;
    let core0 = Arc::new(ModelDevice::new("core0".into(), model, initial_pc));
    guest.devices.insert("core0".into(), core0.clone());
    ObjectStore::global().insert(core0.clone());
    ObjectStore::global().insert_alias(core0.id(), "core0".into());

    // gic
    let gic = Arc::new(GlobalInterruptController::new());
    guest.devices.insert("gic0".into(), gic.clone());
    ObjectStore::global().insert(gic.clone());
    ObjectStore::global().insert_alias(gic.id(), "gic0".into());

    let (cpu, distributor) = GlobalInterruptController::as_interfaces(gic.clone());
    attach_mmap_device(guest, "gic0_cpu".into(), cpu, "as0".into(), 0x0801_0000);
    attach_mmap_device(
        guest,
        "gic0_distributor".into(),
        distributor,
        "as0".into(),
        0x0800_0000,
    );

    // serial
    let pl011 = Arc::new(Pl011::new(66, gic.clone()));
    guest.devices.insert("serial".into(), pl011.clone());
    ObjectStore::global().insert(pl011.clone());
    ObjectStore::global().insert_alias(pl011.id(), "serial".into());
    attach_mmap_device(guest, "serial".into(), pl011, "as0".into(), 0x0900_0000);

    let temp_exec_ctx = Box::new(GuestExecutionContext {
        current_address_space: guest
            .address_spaces
            .get_mut(&("as0".into()))
            .unwrap()
            .as_mut() as *mut AddressSpace,
        interrupt_pending: AtomicU64::new(0),
        unprivileged_access: 0,
    });

    log::debug!("activating guest execution context");
    temp_exec_ctx.activate();

    crate::tests::run(test_config);

    // load data
    let data = guest_data.read_to_vec("/simbench").unwrap();
    ElfBinary::new(&data)
        .unwrap()
        .load(&mut DirectElfLoader)
        .unwrap();

    // go go go (start all devices)
    log::warn!("starting guest");

    for (_, device) in guest
        .devices
        .iter()
        .filter(|(name, _)| **name != InternedString::from_static("core0"))
    {
        device.start();
    }

    guest
        .devices
        .get(&InternedString::from_static("core0"))
        .unwrap()
        .start();
}

fn attach_sysreg_device(device: Arc<dyn Device>, sysregs: BTreeMap<InternedString, [u64; 5]>) {
    let reg_map_device = ObjectStore::global()
        .get_register_mapped_device(device.id())
        .unwrap();

    sysregs
        .iter()
        .map(|(_, [op0, op1, crn, crm, op2])| encode_sysreg_id(*op0, *op1, *crn, *crm, *op2))
        .for_each(|id| {
            sysreg_helpers::register_device(id, reg_map_device.clone());
        });
}

fn attach_mmap_device(
    guest: &mut Guest,
    device_name: InternedString,
    device: Arc<dyn MemoryMappedDevice>,
    address_space: InternedString,
    base: u64,
) {
    if let Some(addrspace) = guest.address_spaces.get_mut(&address_space) {
        addrspace.add_region(AddressSpaceRegion::new(
            device_name,
            base,
            device.address_space_size(),
            memory::AddressSpaceRegionKind::IO(device),
        ));
    } else {
        panic!(
            "address space {} not configured for attaching device {}",
            address_space, device_name
        );
    }
}

struct DirectElfLoader;

impl ElfLoader for DirectElfLoader {
    fn allocate(&mut self, _header: ProgramHeader) -> Result<(), ElfLoaderErr> {
        Ok(())
    }

    fn load(
        &mut self,
        _flags: elfloader::Flags,
        base: elfloader::VAddr,
        region: &[u8],
    ) -> Result<(), ElfLoaderErr> {
        let base_ptr = base as *mut u8;
        unsafe { ptr::copy(region.as_ptr(), base_ptr, region.len()) };
        Ok(())
    }

    fn relocate(&mut self, _entry: RelocationEntry) -> Result<(), ElfLoaderErr> {
        todo!()
    }
}
