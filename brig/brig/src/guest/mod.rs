use {
    crate::{
        guest::{
            devices::{
                arm::{
                    a9gic::GlobalInterruptController,
                    generic_timer::GenericTimer,
                    mmu::{AT_S1E1R, DC_ZVA, at_s1e1r_handler, dc_zva_handler},
                },
                primecell::pl011::Pl011,
                virtio::devices::block::VirtioBlock,
            },
            models::ModelDevice,
        },
        host::fs::Filesystem,
    },
    alloc::{boxed::Box, collections::BTreeMap, sync::Arc},
    brig_common::{
        GuestExecutionContext,
        device::{Device, MemoryMappedDevice, RegisterMappedDevice},
        memory::{AddressSpace, AddressSpaceRegion, AddressSpaceRegionKind},
        sysreg_helpers::{self, encode_sysreg_id},
    },
    common::{TestConfig, intern::InternedString},
    core::{panic, ptr, sync::atomic::AtomicU64},
    elfloader::{ElfLoader, ElfLoaderErr, ProgramHeader, RelocationEntry},
    embedded_time::duration::Nanoseconds,
    spin::{Mutex, Once},
};

pub mod devices;
pub mod memory;
pub mod models;
mod tests;

pub static GUEST: Mutex<Option<Arc<Guest>>> = Mutex::new(None);

pub struct Guest {
    pub address_spaces: BTreeMap<InternedString, Box<AddressSpace>>,
    pub core: Arc<ModelDevice>,
    pub devices: BTreeMap<InternedString, Arc<dyn Device>>,
}

impl Guest {
    pub fn new(core: Arc<ModelDevice>) -> Self {
        Self {
            address_spaces: Default::default(),
            core,
            devices: Default::default(),
        }
    }
}

pub fn run_guest<F: FnOnce()>(mut guest: Guest, f: F) {
    let current_address_space = guest
        .address_spaces
        .get_mut(&("as0".into()))
        .unwrap()
        .as_mut() as *mut AddressSpace;

    *GUEST.lock() = Some(Arc::new(guest));

    let temp_exec_ctx = Box::new(GuestExecutionContext {
        current_address_space,
        interrupt_pending: AtomicU64::new(0),
        unprivileged_access: 0,
    });

    log::debug!("activating guest execution context");
    temp_exec_ctx.activate();

    f()
}

/// Super hacky way of getting the currently executing `ModelDevice`
pub fn get_current_guest() -> Arc<Guest> {
    try_get_current_guest().unwrap()
}

/// Super hacky way of getting the currently executing `ModelDevice`
///
/// "and_then" version breaks:(
pub fn try_get_current_guest() -> Option<Arc<Guest>> {
    GUEST.lock().as_ref().map(|g| g.clone())
}

/// Start guest emulation
pub fn start<FS: Filesystem>(guest_data: &mut FS) {
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

    for (_, device) in get_current_guest()
        .devices
        .iter()
        .filter(|(name, _)| **name != InternedString::from_static("core0"))
    {
        device.start();
    }

    get_current_guest().core.start();
}

pub fn _simbench_platform() -> Guest {
    sysreg_helpers::register_fn(AT_S1E1R, at_s1e1r_handler);
    sysreg_helpers::register_fn(DC_ZVA, dc_zva_handler);

    // core
    let model = models::get("aarch64").unwrap();
    let initial_pc = 0x4000_06b0;
    let core0 = Arc::new(ModelDevice::new("core0".into(), model, initial_pc));
    core0.register_file.write::<u8>("PSTATE_EL", 1);
    core0.register_file.write::<u64>("SCR_EL3_bits", 0x430);

    let mut guest = Guest::new(core0);

    // create memory
    let mut addrspace = AddressSpace::new();
    addrspace.add_region(AddressSpaceRegion::new(
        "ram0".into(),
        0x4000_0000,
        512 * 1024 * 1024,
        AddressSpaceRegionKind::Ram,
    ));
    addrspace.add_region(AddressSpaceRegion::new(
        "ram1".into(),
        0x8000_0000,
        1 * 1024 * 1024 * 1024,
        AddressSpaceRegionKind::Ram,
    ));
    guest
        .address_spaces
        .insert("as0".into(), Box::new(addrspace));

    // gic
    let gic = Arc::new(GlobalInterruptController::new());
    guest.devices.insert("gic0".into(), gic.clone());

    let (cpu, distributor) = GlobalInterruptController::as_interfaces(gic.clone());
    attach_mmap_device(
        &mut guest,
        "gic0_cpu".into(),
        cpu,
        "as0".into(),
        0x0801_0000,
    );
    attach_mmap_device(
        &mut guest,
        "gic0_distributor".into(),
        distributor,
        "as0".into(),
        0x0800_0000,
    );

    // serial
    let pl011 = Arc::new(Pl011::new(66, gic.clone()));
    guest.devices.insert("serial".into(), pl011.clone());
    attach_mmap_device(
        &mut guest,
        "serial".into(),
        pl011,
        "as0".into(),
        0x0900_0000,
    );

    guest
}

pub fn linux_platform() -> Guest {
    sysreg_helpers::register_fn(AT_S1E1R, at_s1e1r_handler);
    sysreg_helpers::register_fn(DC_ZVA, dc_zva_handler);

    // core
    let model = models::get("aarch64").unwrap();
    let initial_pc = 0x8000_0000;
    let core0 = Arc::new(ModelDevice::new("core0".into(), model, initial_pc));

    let mut guest = Guest::new(core0);

    // create memory
    let mut addrspace = AddressSpace::new();
    addrspace.add_region(AddressSpaceRegion::new(
        "ram0".into(),
        0x8000_0000,
        1024 * 1024 * 1024,
        AddressSpaceRegionKind::Ram,
    ));
    addrspace.add_region(AddressSpaceRegion::new(
        "ram1".into(),
        0xdead_b000,
        4096,
        AddressSpaceRegionKind::Ram,
    ));
    addrspace.add_region(AddressSpaceRegion::new(
        "ram2".into(),
        0x1300_0000,
        0x10_0000,
        AddressSpaceRegionKind::Ram,
    ));
    addrspace.add_region(AddressSpaceRegion::new(
        "ram3".into(),
        0x10_ffff_8180,
        0xe80,
        AddressSpaceRegionKind::Ram,
    ));
    guest
        .address_spaces
        .insert("as0".into(), Box::new(addrspace));

    // gic
    let gic = Arc::new(GlobalInterruptController::new());
    guest.devices.insert("gic0".into(), gic.clone());

    let (cpu, distributor) = GlobalInterruptController::as_interfaces(gic.clone());
    attach_mmap_device(
        &mut guest,
        "gic0_cpu".into(),
        cpu,
        "as0".into(),
        0x2c00_2000,
    );
    attach_mmap_device(
        &mut guest,
        "gic0_distributor".into(),
        distributor,
        "as0".into(),
        0x2c00_1000,
    );

    // serial
    let pl011 = Arc::new(Pl011::new(66, gic.clone()));
    guest.devices.insert("serial".into(), pl011.clone());

    attach_mmap_device(
        &mut guest,
        "serial".into(),
        pl011,
        "as0".into(),
        0x3c00_0000,
    );

    // block
    let block = Arc::new(VirtioBlock::new(64, gic.clone()));
    guest.devices.insert("block".into(), block.clone());

    attach_mmap_device(&mut guest, "block".into(), block, "as0".into(), 0x3d00_0000);

    // timer
    let timer = Arc::new(GenericTimer::new(gic.clone(), 27, Nanoseconds::new(1_000)));
    guest.devices.insert("timer".into(), timer.clone());

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

    guest
}

fn attach_sysreg_device(
    device: Arc<dyn RegisterMappedDevice>,
    sysregs: BTreeMap<InternedString, [u64; 5]>,
) {
    sysregs
        .iter()
        .map(|(_, [op0, op1, crn, crm, op2])| encode_sysreg_id(*op0, *op1, *crn, *crm, *op2))
        .for_each(|id| {
            sysreg_helpers::register_device(id, device.clone());
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
            AddressSpaceRegionKind::IO(device),
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
