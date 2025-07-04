use {
    crate::host::objects::device::Device,
    alloc::{collections::btree_map::BTreeMap, sync::Arc},
    common::intern::InternedString,
    linkme::distributed_slice,
};

pub mod arm;
pub mod primecell;
pub mod virtio;

//pub type DeviceConfig =

#[distributed_slice]
pub static DEVICE_FACTORIES: [(
    &str,
    fn(&BTreeMap<InternedString, InternedString>) -> Arc<dyn Device>,
)];

pub fn create_device(
    device_kind: InternedString,
    config: &BTreeMap<InternedString, InternedString>,
) -> Option<Arc<dyn Device>> {
    DEVICE_FACTORIES
        .iter()
        .find(|(n, _)| *n == device_kind.as_ref())
        .map(|(_, f)| f(config))
}
