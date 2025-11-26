use {
    crate::devices::{ReadRegister, VIRTIO_DEV_BLK, Virtio, WriteRegister},
    alloc::sync::Arc,
    common::device::{Device, IrqController, MemoryMappedDevice},
    kernel::{
        devices::{BlockDevice, manager::SharedDeviceManager},
        util::any_as_u8_slice,
    },
    spin::Mutex,
    virtio_bindings::virtio_blk::virtio_blk_config,
};

pub struct VirtioBlock<S, ReadCallback, WriteCallback> {
    virtio: Mutex<Virtio<S, ReadCallback, WriteCallback>>,
    config: virtio_blk_config,
}

pub fn new_virtio_block<B: BlockDevice>(
    irq_line: usize,
    controller: Arc<dyn IrqController>,
    // B is the callback state (S in lower levels)
    block: B,
) -> VirtioBlock<B, impl Fn(&mut B, &mut [u8], usize) + Send, impl Fn(&mut B, &[u8], usize) + Send>
{
    let mut config = virtio_blk_config::default();

    // in 512 byte sectors
    assert_eq!(block.block_size(), 512);
    config.capacity = u64::try_from(block.size() / block.block_size()).unwrap();
    config.blk_size = u32::try_from(block.block_size()).unwrap();

    let celf = VirtioBlock {
        virtio: Mutex::new(Virtio::new(
            VIRTIO_DEV_BLK,
            irq_line,
            controller,
            block,
            read_callback,
            write_callback,
        )),
        config,
    };

    celf.virtio.lock().set_host_feature(6);
    celf.virtio.lock().set_host_feature(32);

    celf
}

impl<
    B: BlockDevice,
    ReadCallback: Fn(&mut B, &mut [u8], usize) + Send,
    WriteCallback: Fn(&mut B, &[u8], usize) + Send,
> Device for VirtioBlock<B, ReadCallback, WriteCallback>
{
    fn start(&self) {}
    fn stop(&self) {}
}

impl<
    B: BlockDevice,
    ReadCallback: Fn(&mut B, &mut [u8], usize) + Send,
    WriteCallback: Fn(&mut B, &[u8], usize) + Send,
> MemoryMappedDevice for VirtioBlock<B, ReadCallback, WriteCallback>
{
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

fn read_callback<B: BlockDevice>(blk: &mut B, dest: &mut [u8], sector: usize) {
    log::trace!(
        "reading {} bytes @ {sector:x} into {:x}",
        dest.len(),
        dest.as_ptr() as u64
    );

    assert_eq!(blk.block_size(), 512);
    assert_eq!(dest.len() % 512, 0);

    // reading only one chunk at a time fixes the memory corruption bug, but I don't
    // know why `virtio-drivers` crate's read method is supposed to work on
    // multiple blocks (and we never had any issues loading very large model files,
    // plugins, etc)
    dest.chunks_mut(512)
        .enumerate()
        .for_each(|(i, chunk)| blk.read(chunk, sector + i).unwrap());
}

fn write_callback<B: BlockDevice>(blk: &mut B, source: &[u8], sector: usize) {
    log::debug!("writing {} bytes @ {sector:x}", source.len());

    assert_eq!(blk.block_size(), 512);
    assert_eq!(source.len() % 512, 0);

    // see comment in read_callback
    source
        .chunks(512)
        .enumerate()
        .for_each(|(i, chunk)| blk.write(chunk, sector + i).unwrap());
}
