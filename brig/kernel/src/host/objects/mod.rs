use {
    crate::host::objects::{
        device::{Device, MemoryMappedDevice, RegisterMappedDevice},
        irq::IrqController,
        tickable::Tickable,
    },
    alloc::{fmt, sync::Arc},
    common::{
        hashmap::{HashMap, HashSet},
        intern::InternedString,
    },
    core::{any::Any, fmt::Display, sync::atomic::AtomicU64},
    spin::{Lazy, Mutex},
};

pub mod device;
pub mod irq;
pub mod object_store;
pub mod tickable;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ObjectId(u64);

impl Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Object({:x})", self.0)
    }
}

impl ObjectId {
    pub fn new() -> Self {
        Self(
            ObjectStore::global()
                .id_counter
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed),
        )
    }
}

pub trait Object:
    Send
    + Sync
    + ToDevice
    + ToMemoryMappedDevice
    + ToRegisterMappedDevice
    + ToTickable
    + ToIrqController
    + Any
{
    fn id(&self) -> ObjectId;
}

pub trait ToDevice {
    fn to_device<'a>(self: Arc<Self>) -> Option<Arc<dyn Device + 'a>>
    where
        Self: 'a,
    {
        None
    }
}

impl<T: Device> ToDevice for T {
    fn to_device<'a>(self: Arc<Self>) -> Option<Arc<dyn Device + 'a>>
    where
        Self: 'a,
    {
        Some(self)
    }
}

pub trait ToMemoryMappedDevice {
    fn to_memory_mapped_device<'a>(self: Arc<Self>) -> Option<Arc<dyn MemoryMappedDevice + 'a>>
    where
        Self: 'a,
    {
        None
    }
}

impl<T: MemoryMappedDevice> ToMemoryMappedDevice for T {
    fn to_memory_mapped_device<'a>(self: Arc<Self>) -> Option<Arc<dyn MemoryMappedDevice + 'a>>
    where
        Self: 'a,
    {
        Some(self)
    }
}

pub trait ToRegisterMappedDevice {
    fn to_register_mapped_device<'a>(self: Arc<Self>) -> Option<Arc<dyn RegisterMappedDevice + 'a>>
    where
        Self: 'a,
    {
        None
    }
}

impl<T: RegisterMappedDevice> ToRegisterMappedDevice for T {
    fn to_register_mapped_device<'a>(self: Arc<Self>) -> Option<Arc<dyn RegisterMappedDevice + 'a>>
    where
        Self: 'a,
    {
        Some(self)
    }
}

pub trait ToTickable {
    fn to_tickable<'a>(self: Arc<Self>) -> Option<Arc<dyn Tickable + 'a>>
    where
        Self: 'a,
    {
        None
    }
}

impl<T: Tickable> ToTickable for T {
    fn to_tickable<'a>(self: Arc<Self>) -> Option<Arc<dyn Tickable + 'a>>
    where
        Self: 'a,
    {
        Some(self)
    }
}

pub trait ToIrqController {
    fn to_irq_controller<'a>(self: Arc<Self>) -> Option<Arc<dyn IrqController + 'a>>
    where
        Self: 'a,
    {
        None
    }
}

impl<T: IrqController> ToIrqController for T {
    fn to_irq_controller<'a>(self: Arc<Self>) -> Option<Arc<dyn IrqController + 'a>>
    where
        Self: 'a,
    {
        Some(self)
    }
}

// macro_rules! object_type {
//     ($type_name:ident, $to_name:ident) => {
//         pub trait concat_idents!(To, $type_name) {
//             fn to_$to_name<'a>(self: Arc<Self>) -> Option<Arc<dyn $type_name
// + 'a>>             where Self: 'a, { None } }

//         impl<T: $type_name> To$type_name for T {
//             fn to_$to_name<'a>(self: Arc<Self>) -> Option<Arc<dyn $type_name
// + 'a>>             where Self: 'a, { Some(self) } } };
// }

// object_type!(IrqController, irq_controller);

static STORE: Lazy<ObjectStore> = Lazy::new(|| ObjectStore::new());

pub struct ObjectStore {
    id_counter: AtomicU64,
    state: Mutex<ObjectStoreState>,
}

#[derive(Default)]
struct ObjectStoreState {
    objects: HashMap<ObjectId, Arc<dyn Object>>,
    aliases: HashMap<InternedString, ObjectId>,
    devices: HashSet<ObjectId>,
    memory_mapped_devices: HashSet<ObjectId>,
    register_mapped_devices: HashSet<ObjectId>,
    tickables: HashSet<ObjectId>,
    irq_controllers: HashSet<ObjectId>,
}

impl ObjectStore {
    fn new() -> Self {
        Self {
            id_counter: AtomicU64::new(0),
            state: Mutex::new(ObjectStoreState::default()),
        }
    }

    pub fn global() -> &'static ObjectStore {
        &*STORE
    }

    pub fn insert(&self, object: Arc<dyn Object>) {
        let mut guard = self.state.lock();

        if object.clone().to_device().is_some() {
            guard.devices.insert(object.id());
        }
        if object.clone().to_memory_mapped_device().is_some() {
            guard.memory_mapped_devices.insert(object.id());
        }

        if object.clone().to_register_mapped_device().is_some() {
            guard.register_mapped_devices.insert(object.id());
        }

        if object.clone().to_tickable().is_some() {
            guard.tickables.insert(object.id());
        }

        if object.clone().to_irq_controller().is_some() {
            guard.irq_controllers.insert(object.id());
        }

        guard.objects.insert(object.id(), object);
    }

    pub fn get(&self, id: ObjectId) -> Option<Arc<dyn Object>> {
        self.state.lock().objects.get(&id).cloned()
    }

    pub fn get_device(&self, id: ObjectId) -> Option<Arc<dyn Device>> {
        let state = self.state.lock();

        if state.devices.contains(&id) {
            Some(state.objects.get(&id).unwrap().clone().to_device().unwrap())
        } else {
            None
        }
    }

    pub fn get_memory_mapped_device(&self, id: ObjectId) -> Option<Arc<dyn MemoryMappedDevice>> {
        let state = self.state.lock();

        if state.memory_mapped_devices.contains(&id) {
            Some(
                state
                    .objects
                    .get(&id)
                    .unwrap()
                    .clone()
                    .to_memory_mapped_device()
                    .unwrap(),
            )
        } else {
            None
        }
    }

    pub fn get_register_mapped_device(
        &self,
        id: ObjectId,
    ) -> Option<Arc<dyn RegisterMappedDevice>> {
        let state = self.state.lock();

        if state.register_mapped_devices.contains(&id) {
            Some(
                state
                    .objects
                    .get(&id)
                    .unwrap()
                    .clone()
                    .to_register_mapped_device()
                    .unwrap(),
            )
        } else {
            None
        }
    }

    pub fn get_tickable(&self, id: ObjectId) -> Option<Arc<dyn Tickable>> {
        let state = self.state.lock();

        if state.tickables.contains(&id) {
            Some(
                state
                    .objects
                    .get(&id)
                    .unwrap()
                    .clone()
                    .to_tickable()
                    .unwrap(),
            )
        } else {
            None
        }
    }

    pub fn insert_alias(&self, id: ObjectId, name: InternedString) {
        self.state.lock().aliases.insert(name, id);
    }

    pub fn lookup_by_alias(&self, name: InternedString) -> Option<ObjectId> {
        self.state.lock().aliases.get(&name).copied()
    }

    pub fn get_irq_controller(&self, id: ObjectId) -> Option<Arc<dyn IrqController>> {
        let state = self.state.lock();

        if state.irq_controllers.contains(&id) {
            Some(
                state
                    .objects
                    .get(&id)
                    .unwrap()
                    .clone()
                    .to_irq_controller()
                    .unwrap_or_else(|| panic!("{id:?} in irq_controllers hash set but `to_irq_controller` returned None")),
            )
        } else {
            None
        }
    }
}
