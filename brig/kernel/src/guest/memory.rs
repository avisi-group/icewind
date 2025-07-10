use {
    crate::host::objects::device::MemoryMappedDevice,
    alloc::{collections::BTreeMap, sync::Arc},
    common::intern::InternedString,
    core::{
        fmt::Display,
        mem::{MaybeUninit, size_of},
        slice,
    },
};

#[derive(Debug)]
pub struct AddressSpace {
    regions: BTreeMap<u64, AddressSpaceRegion>,
}

impl AddressSpace {
    pub fn new() -> Self {
        Self {
            regions: BTreeMap::new(),
        }
    }

    pub fn add_region(&mut self, region: AddressSpaceRegion) {
        log::trace!("addr-space: adding region {}", region);
        self.regions.insert(region.base, region);
    }

    pub fn find_region(&self, address: u64) -> Option<&AddressSpaceRegion> {
        let candidate = self
            .regions
            .upper_bound(core::ops::Bound::Included(&address))
            .prev()?;

        if address >= candidate.1.base && address < (candidate.1.base + candidate.1.size) {
            Some(candidate.1)
        } else {
            None
        }
    }
}

pub trait IoMemoryHandler {
    fn read_fixed<G: MemoryMappedDevice, T: Sized>(device: G, offset: u64) -> T {
        let mut t = MaybeUninit::<T>::uninit();
        let buf = unsafe { slice::from_raw_parts_mut(t.as_mut_ptr() as *mut u8, size_of::<T>()) };
        device.read(offset, buf);

        unsafe { t.assume_init() }
    }
}

pub enum AddressSpaceRegionKind {
    Ram,
    IO(Arc<dyn MemoryMappedDevice>),
}

impl core::fmt::Debug for AddressSpaceRegionKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Ram => write!(f, "ram"),
            Self::IO(_) => write!(f, "io"),
        }
    }
}

#[derive(Debug)]
pub struct AddressSpaceRegion {
    name: InternedString,
    base: u64,
    size: u64,
    kind: AddressSpaceRegionKind,
}

impl Display for AddressSpaceRegion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "name={}, base={:x}, size={:x}",
            self.name, self.base, self.size
        )
    }
}

impl AddressSpaceRegion {
    pub fn new(name: InternedString, base: u64, size: u64, kind: AddressSpaceRegionKind) -> Self {
        Self {
            name,
            base,
            size,
            kind,
        }
    }

    pub fn name(&self) -> InternedString {
        self.name
    }

    pub fn kind(&self) -> &AddressSpaceRegionKind {
        &self.kind
    }

    pub fn base(&self) -> u64 {
        self.base
    }

    pub fn size(&self) -> u64 {
        self.size
    }
}
