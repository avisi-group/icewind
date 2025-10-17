use {
    crate::{devices::SharedDevice, rand::new_uuid_v4},
    alloc::{borrow::ToOwned, collections::BTreeMap, string::String, vec::Vec},
    spin::{Mutex, Once},
    uuid::Uuid,
};

static DEVICE_MANAGER: Once<SharedDeviceManager> = Once::INIT;

pub fn init() {
    DEVICE_MANAGER.call_once(SharedDeviceManager::default);
}

#[derive(Debug, Default)]
pub struct SharedDeviceManager {
    inner: Mutex<InnerDeviceManager>,
}

#[derive(Debug, Default)]
pub struct InnerDeviceManager {
    devices: BTreeMap<Uuid, SharedDevice>,
    aliases: BTreeMap<String, Uuid>,
}

impl SharedDeviceManager {
    pub fn get() -> &'static SharedDeviceManager {
        DEVICE_MANAGER.get().unwrap()
    }

    pub fn register_device(&self, device: SharedDevice) -> Uuid {
        let id = new_uuid_v4();
        log::debug!("registering device {id:?}: {device:?}");
        let res = self.inner.lock().devices.insert(id, device);

        if res.is_some() {
            unreachable!("uuids should be unique")
        }

        id
    }

    pub fn add_alias<S: AsRef<str>>(&self, device: Uuid, alias: S) {
        self.inner
            .lock()
            .aliases
            .insert(alias.as_ref().to_owned(), device);

        log::trace!("added alias {:?} for {device}", alias.as_ref());
    }

    pub fn _remove_alias<S: AsRef<str>>(&self, alias: S) {
        self.inner.lock().aliases.remove(alias.as_ref());
    }

    /// This could be more efficient by returning an iterator, but would require
    /// holding the lock and possibly risking deadlock.
    pub fn _get_aliases_by_device(&self, device: Uuid) -> Vec<String> {
        self.inner
            .lock()
            .aliases
            .iter()
            .filter(|(_, v)| **v == device)
            .map(|(k, _)| k)
            .cloned()
            .collect()
    }

    pub fn lookup_alias<S: AsRef<str>>(&self, alias: S) -> Option<Uuid> {
        self.inner.lock().aliases.get(alias.as_ref()).copied()
    }

    pub fn _get_device_by_id(&self, id: Uuid) -> Option<SharedDevice> {
        self.inner.lock().devices.get(&id).cloned()
    }

    pub fn get_device_by_alias<S: AsRef<str>>(&self, alias: S) -> Option<SharedDevice> {
        let id = &self.lookup_alias(alias)?;
        self.inner.lock().devices.get(id).cloned()
    }
}
