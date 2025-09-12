use {
    crate::{
        guest::devices::virtio::devices::{ReadRegister, VIRTIO_DEV_BLK, Virtio, WriteRegister},
        host::{
            devices::manager::SharedDeviceManager,
            objects::{
                Object, ObjectId, ObjectStore, ToIrqController, ToRegisterMappedDevice, ToTickable,
                device::{Device, MemoryMappedDevice},
                irq::IrqController,
            },
        },
        util::any_as_u8_slice,
    },
    alloc::{collections::BTreeMap, sync::Arc},
    common::intern::InternedString,
    proc_macro_lib::guest_device_factory,
    spin::Mutex,
    virtio_bindings::virtio_blk::virtio_blk_config,
};

pub struct VirtioBlock {
    id: ObjectId,
    virtio: Mutex<Virtio>,
    config: virtio_blk_config,
}

impl VirtioBlock {
    pub fn new(irq_line: usize, controller: Arc<dyn IrqController>) -> Self {
        let mut celf = Self {
            id: ObjectId::new(),
            virtio: Mutex::new(Virtio::new(
                1,
                VIRTIO_DEV_BLK,
                irq_line,
                controller,
                read_callback,
                write_callback,
            )),
            config: virtio_blk_config::default(),
        };

        // in 512 byte sectors
        //  62914560 b
        // /home/fm208/Documents/Sync/icewind/brig-cli/guest_data/rootfs.ext2
        celf.config.capacity = 62914560 / 512; // = 122880
        celf.config.blk_size = 512;

        celf.virtio.lock().set_host_feature(6);
        celf.virtio.lock().set_host_feature(32);

        celf
    }
}

impl Object for VirtioBlock {
    fn id(&self) -> ObjectId {
        self.id
    }
}

impl ToTickable for VirtioBlock {}
impl ToRegisterMappedDevice for VirtioBlock {}
impl ToIrqController for VirtioBlock {}

impl Device for VirtioBlock {
    fn start(&self) {}
    fn stop(&self) {}
}

impl MemoryMappedDevice for VirtioBlock {
    fn address_space_size(&self) -> u64 {
        0x1000
    }

    fn read(&self, offset: u64, dest: &mut [u8]) {
        if offset <= 0xff {
            let register = ReadRegister::from_offset(offset);
            let read = self.virtio.lock().read_register(register);
            dest.copy_from_slice(&read.to_le_bytes());
        } else {
            let config_offset = usize::try_from(offset - 0x100).unwrap();

            log::debug!("reading config @ {config_offset:x}");

            let config = unsafe { any_as_u8_slice(&self.config) };
            let start = config_offset;
            let end = config_offset + dest.len();

            dest.copy_from_slice(&config[start..end]);
        }
    }

    fn write(&self, offset: u64, value: &[u8]) {
        let register = WriteRegister::from_offset(offset);
        let value = u32::from_le_bytes(value.try_into().unwrap());
        self.virtio.lock().write_register(register, value);
    }
}

fn read_callback(dest: &mut [u8], sector: usize) {
    let dev = SharedDeviceManager::get()
        .get_device_by_alias("disk00:04.0")
        .unwrap();
    let mut dev = dev.lock();
    let blk = dev.as_block();

    log::debug!("reading {} bytes @ {sector:x}", dest.len());

    let offset = (sector * 512) / blk.block_size();

    dest.chunks_mut(0x1000)
        .enumerate()
        .for_each(|(chunk_idx, dest_chunk)| {
            blk.read(dest_chunk, offset + (chunk_idx * 0x1000)).unwrap();
        });
}

fn write_callback(source: &[u8], sector: usize) {
    let dev = SharedDeviceManager::get()
        .get_device_by_alias("disk00:04.0")
        .unwrap();
    let mut dev = dev.lock();
    let blk = dev.as_block();

    log::debug!("writing {} bytes @ {sector:x}", source.len());

    let offset = (sector * 512) / blk.block_size();
    blk.write(source, offset).unwrap();
}
