use {
    crate::{
        guest::{
            config::{DeviceAttachment, LoadKind},
            memory::{AddressSpace, AddressSpaceRegion},
        },
        host::{
            dbt::sysreg_helpers::{self, encode_sysreg_id},
            fs::Filesystem,
            objects::{ObjectStore, device::Device},
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
    //check each connected block device for guest config
    let config = config::load_from_fs(guest_data).unwrap();

    log::debug!("got config: {:#x?}", config);

    unsafe { GUEST.call_once(Guest::new) };
    let guest = unsafe { GUEST.get_mut() }.unwrap();

    // create memory
    for (name, regions) in config.memory {
        let mut addrspace = AddressSpace::new();

        for (name, region) in regions {
            addrspace.add_region(AddressSpaceRegion::new(
                name,
                region.start,
                region.end - region.start,
                memory::AddressSpaceRegionKind::Ram,
            ));
        }

        guest.address_spaces.insert(name, Box::new(addrspace));
    }

    // create devices, including cores
    for device_config in config.devices {
        let device = devices::create_device(device_config.kind, &device_config.extra)
            .unwrap_or_else(|| {
                panic!(
                    "failed to create device {:?} with config {:?}",
                    device_config.kind, device_config.extra
                )
            });

        guest.devices.insert(device_config.name, device.clone());
        ObjectStore::global().insert(device.clone());
        ObjectStore::global().insert_alias(device.id(), device_config.name);

        // locate address space for attachment, if any
        match device_config.attach {
            Some(DeviceAttachment::Memory {
                address_space,
                base,
            }) => {
                let mem_map_device = ObjectStore::global()
                    .get_memory_mapped_device(device.id())
                    .unwrap();

                if let Some(addrspace) = guest.address_spaces.get_mut(&address_space) {
                    addrspace.add_region(AddressSpaceRegion::new(
                        device_config.name,
                        base,
                        mem_map_device.address_space_size(),
                        memory::AddressSpaceRegionKind::IO(mem_map_device.clone()),
                    ));
                } else {
                    panic!(
                        "address space {} not configured for attaching device {}",
                        address_space, device_config.name
                    );
                }
            }
            Some(DeviceAttachment::SysReg(sysregs)) => {
                let reg_map_device = ObjectStore::global()
                    .get_register_mapped_device(device.id())
                    .unwrap();

                sysregs
                    .iter()
                    .map(|(_, [op0, op1, crn, crm, op2])| {
                        encode_sysreg_id(*op0, *op1, *crn, *crm, *op2)
                    })
                    .for_each(|id| {
                        sysreg_helpers::register_device(id, reg_map_device.clone());
                    });
            }
            None => (),
        }
    }

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

    {
        for load in config.load {
            let data = guest_data.read_to_vec(&load.path).unwrap();

            match load.kind {
                LoadKind::Elf => {
                    log::warn!("loading ELF {:?}", load.path);
                    let elf = ElfBinary::new(&data).unwrap();
                    elf.load(&mut DirectElfLoader).unwrap();
                } /*       let offset = load.offset.unwrap_or(0);
                   * let len = load
                   *     .size
                   *     .map(|s| usize::try_from(s).unwrap())
                   *     .unwrap_or(data.len()); */

                  /* log::warn!(
                   *     "loading {len} bytes of {:?} (+{offset:x}) to {:p}",
                   *     load.path,
                   *     pointer
                   * ); */

                  /* unsafe {
                   *     ptr::copy(
                   *         data.as_ptr().add(usize::try_from(offset).unwrap()),
                   *         pointer,
                   *         len,
                   *     );
                   * }
                   * } */
            }
        }
    }

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

    fn relocate(&mut self, entry: RelocationEntry) -> Result<(), ElfLoaderErr> {
        todo!()
    }
}
