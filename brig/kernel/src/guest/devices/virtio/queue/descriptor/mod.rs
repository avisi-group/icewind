//! Descriptor types for virtio queue.

pub mod packed;
pub mod split;

/// a virtio descriptor
#[deprecated = "Descriptor has been deprecated. Please use RawDescriptor"]
pub type Descriptor = RawDescriptor;

/// A virtio descriptor's layout constraints with C representation.
/// This is a unified representation of the memory layout order
/// for packed descriptors and split descriptors.
/// This type corresponds to struct virtq_desc, see:
/// https://docs.oasis-open.org/virtio/virtio/v1.3/csd01/virtio-v1.3-csd01.html#x1-720008
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RawDescriptor(u64, u32, u16, u16);

// // SAFETY: This is safe because `Descriptor` contains only wrappers over POD
// // types and all accesses through safe `vm-memory` API will validate any
// garbage // that could be included in there.
// unsafe impl ByteValued for RawDescriptor {}

impl From<split::Descriptor> for RawDescriptor {
    fn from(desc: split::Descriptor) -> Self {
        RawDescriptor(desc.addr(), desc.len(), desc.flags(), desc.next())
    }
}

impl From<packed::Descriptor> for RawDescriptor {
    fn from(desc: packed::Descriptor) -> Self {
        RawDescriptor(desc.addr(), desc.len(), desc.id(), desc.flags())
    }
}

impl From<RawDescriptor> for split::Descriptor {
    fn from(desc: RawDescriptor) -> split::Descriptor {
        split::Descriptor::new(desc.0.into(), desc.1.into(), desc.2.into(), desc.3.into())
    }
}

impl From<RawDescriptor> for packed::Descriptor {
    fn from(desc: RawDescriptor) -> packed::Descriptor {
        packed::Descriptor::new(desc.0.into(), desc.1.into(), desc.2.into(), desc.3.into())
    }
}

mod tests {
    use {
        super::{RawDescriptor, packed, split},
        proc_macro_lib::ktest,
    };

    #[ktest]
    fn test_desc_from_split() {
        let split_desc = split::Descriptor::new(1, 2, 3, 4);
        let desc = RawDescriptor::from(split_desc);
        assert_eq!(split_desc.addr(), desc.0);
        assert_eq!(split_desc.len(), desc.1);
        assert_eq!(split_desc.flags(), desc.2);
        assert_eq!(split_desc.next(), desc.3);
    }

    #[ktest]
    fn test_split_from_desc() {
        let desc = RawDescriptor(1, 2, 3, 4);
        let split_desc = split::Descriptor::from(desc);
        assert_eq!(split_desc.addr(), desc.0);
        assert_eq!(split_desc.len(), desc.1);
        assert_eq!(split_desc.flags(), desc.2);
        assert_eq!(split_desc.next(), desc.3);
    }

    #[ktest]
    fn test_desc_from_packed() {
        let packed_desc = packed::Descriptor::new(1, 2, 3, 4);
        let desc = RawDescriptor::from(packed_desc);
        assert_eq!(packed_desc.addr(), desc.0);
        assert_eq!(packed_desc.len(), desc.1);
        assert_eq!(packed_desc.id(), desc.2);
        assert_eq!(packed_desc.flags(), desc.3);
    }

    #[ktest]
    fn test_packed_from_desc() {
        let desc = RawDescriptor(1, 2, 3, 4);
        let packed_desc = packed::Descriptor::from(desc);
        assert_eq!(packed_desc.addr(), desc.0);
        assert_eq!(packed_desc.len(), desc.1);
        assert_eq!(packed_desc.id(), desc.2);
        assert_eq!(packed_desc.flags(), desc.3);
    }
}
