use {
    crate::{
        guest::{
            devices::{
                arm::{a9gic::GlobalInterruptController, generic_timer::GenericTimer},
                primecell::pl011::Pl011,
                virtio::devices::block::VirtioBlock,
            },
            memory::{AddressSpace, AddressSpaceRegion},
        },
        host::{
            dbt::{
                models::{self, ModelDevice},
                sysreg_helpers::{self, encode_sysreg_id},
            },
            fs::Filesystem,
            objects::{
                Object, ObjectStore,
                device::{Device, MemoryMappedDevice},
            },
        },
        util::get_current_device,
    },
    alloc::{boxed::Box, collections::BTreeMap, sync::Arc},
    common::{TestConfig, intern::InternedString},
    core::{panic, ptr, sync::atomic::AtomicU64},
    elfloader::{ElfBinary, ElfLoader, ElfLoaderErr, ProgramHeader, RelocationEntry},
    embedded_time::duration::Nanoseconds,
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
pub fn start<FS: Filesystem>(guest_data: &mut FS) {
    unsafe { GUEST.call_once(Guest::new) };
    let guest = unsafe { GUEST.get_mut() }.unwrap();

    linux(guest);
    //simbench(guest);

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

    // load data

    // simbench
    // {
    // ElfBinary::new(&guest_data.read_to_vec("/simbench").unwrap())
    //     .unwrap()
    //     .load(&mut DirectElfLoader)
    //     .unwrap();
    // }

    // linux
    {
        let data = guest_data.read_to_vec("/bootloader.bin").unwrap();
        unsafe { ptr::copy(data.as_ptr(), 0x8000_0000 as *mut u8, data.len()) };
    }
    {
        let data = guest_data.read_to_vec("/sail.dtb").unwrap();
        unsafe { ptr::copy(data.as_ptr(), 0x8100_0000 as *mut u8, data.len()) };
    }
    {
        let data = guest_data.read_to_vec("/Image").unwrap();
        unsafe { ptr::copy(data.as_ptr(), 0x8208_0000 as *mut u8, data.len()) };
    }

    // {
    //     let data = guest_data.read_to_vec("/fuzzrs").unwrap();
    //     let elf = ElfBinary::new(&data).unwrap();

    //     get_current_device()
    //         .register_file
    //         .write("_PC", elf.entry_point());

    //     elf.load(&mut DirectElfLoader).unwrap();
    // }

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

pub fn tests(config: TestConfig) {
    unsafe { GUEST.call_once(Guest::new) };
    let guest = unsafe { GUEST.get_mut() }.unwrap();

    linux(guest);

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

    crate::tests::run(config);
}

fn simbench(guest: &mut Guest) {
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

    core0.register_file.write::<u8>("PSTATE_EL", 1);
    core0.register_file.write::<u64>("SCR_EL3_bits", 0x430);

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
}

fn linux(guest: &mut Guest) {
    // create memory
    let mut addrspace = AddressSpace::new();
    addrspace.add_region(AddressSpaceRegion::new(
        "ram0".into(),
        0x8000_0000,
        1024 * 1024 * 1024,
        memory::AddressSpaceRegionKind::Ram,
    ));
    addrspace.add_region(AddressSpaceRegion::new(
        "ram1".into(),
        0xdead_b000,
        4096,
        memory::AddressSpaceRegionKind::Ram,
    ));
    addrspace.add_region(AddressSpaceRegion::new(
        "ram2".into(),
        0x1300_0000,
        0x10_0000,
        memory::AddressSpaceRegionKind::Ram,
    ));
    addrspace.add_region(AddressSpaceRegion::new(
        "ram3".into(),
        0x10_ffff_8180,
        0xe80,
        memory::AddressSpaceRegionKind::Ram,
    ));
    guest
        .address_spaces
        .insert("as0".into(), Box::new(addrspace));

    // core
    let model = models::get("aarch64").unwrap();
    let initial_pc = 0x8000_0000;
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
    attach_mmap_device(guest, "gic0_cpu".into(), cpu, "as0".into(), 0x2c00_2000);
    attach_mmap_device(
        guest,
        "gic0_distributor".into(),
        distributor,
        "as0".into(),
        0x2c00_1000,
    );

    // serial
    let pl011 = Arc::new(Pl011::new(66, gic.clone()));
    guest.devices.insert("serial".into(), pl011.clone());
    ObjectStore::global().insert(pl011.clone());
    ObjectStore::global().insert_alias(pl011.id(), "serial".into());
    attach_mmap_device(guest, "serial".into(), pl011, "as0".into(), 0x3c00_0000);

    // block
    let block = Arc::new(VirtioBlock::new(64, gic.clone()));
    guest.devices.insert("block".into(), block.clone());
    ObjectStore::global().insert(block.clone());
    ObjectStore::global().insert_alias(block.id(), "block".into());
    attach_mmap_device(guest, "block".into(), block, "as0".into(), 0x3d00_0000);

    // timer
    let timer = Arc::new(GenericTimer::new(gic.clone(), 27, Nanoseconds::new(1_000)));
    guest.devices.insert("timer".into(), timer.clone());
    ObjectStore::global().insert(timer.clone());
    ObjectStore::global().insert_alias(timer.id(), "timer".into());
    let sysregs = [
        ("cntkctl_el1", [3, 0, 14, 1, 0]),
        ("cntfrq_el0", [3, 3, 14, 0, 0]),
        ("cntpct_el0", [3, 3, 14, 0, 1]),
        ("cntvct_el0", [3, 3, 14, 0, 2]),
        ("cntp_tval_el0", [3, 3, 14, 2, 0]),
        ("cntp_ctl_el0", [3, 3, 14, 2, 1]),
        ("cntp_cval_el0", [3, 3, 14, 2, 2]),
        ("cntvoff_el2", [3, 4, 14, 0, 3]),
        ("cntps_tval_el1", [3, 7, 14, 2, 0]),
        ("cntps_ctl_el1", [3, 7, 14, 2, 1]),
        ("cntps_cval_el1", [3, 7, 14, 2, 2]),
        ("cntv_tval_el0", [3, 3, 14, 3, 0]),
        ("cntv_ctl_el0", [3, 3, 14, 3, 1]),
        ("cntv_cval_el0", [3, 3, 14, 3, 2]),
    ]
    .into_iter()
    .map(|(n, i)| (InternedString::from(n), i))
    .collect();

    attach_sysreg_device(timer, sysregs);
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
