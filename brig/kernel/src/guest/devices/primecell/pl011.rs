use {
    crate::host::objects::{
        Object, ObjectId, ObjectStore, ToIrqController, ToRegisterMappedDevice, ToTickable,
        device::{Device, MemoryMappedDevice},
    },
    alloc::{collections::BTreeMap, sync::Arc},
    common::intern::InternedString,
    proc_macro_lib::guest_device_factory,
    spin::Mutex,
};

#[guest_device_factory(pl011)]
fn create_pl011(_config: &BTreeMap<InternedString, InternedString>) -> Arc<dyn Device> {
    let dev = Arc::new(Pl011 {
        id: ObjectId::new(),
        registers: Mutex::new(Registers::new()),
    });

    dev
}

#[derive(Debug)]
struct Pl011 {
    id: ObjectId,
    registers: Mutex<Registers>,
}

impl Object for Pl011 {
    fn id(&self) -> ObjectId {
        self.id
    }
}

impl ToTickable for Pl011 {}
impl ToRegisterMappedDevice for Pl011 {}
impl ToIrqController for Pl011 {}

impl Device for Pl011 {
    fn start(&self) {}
    fn stop(&self) {}
}

impl MemoryMappedDevice for Pl011 {
    fn address_space_size(&self) -> u64 {
        0x1000
    }

    fn read(&self, offset: u64, value: &mut [u8]) {
        self.registers.lock().read(offset, value);
    }

    fn write(&self, offset: u64, value: &[u8]) {
        self.registers.lock().write(offset, value);
    }
}

#[derive(Debug)]
struct Registers {}

impl Registers {
    fn new() -> Self {
        Self {}
    }

    fn write(&mut self, offset: u64, src: &[u8]) {
        match offset {
            0x000 => crate::print!("{}", src[0] as char),
            0x004 => (),
            0x018 => panic!("read only register"),
            0x008..0x014 | 0x01C | 0x04C..0x07C | 0x090..0xFCC => {
                panic!("reserved")
            }
            0x080..0x08C => panic!("reserved for test purposes"),
            0xFD0..0xFDC => panic!("reserved for future ID expansion"),
            offset => unreachable!("got write on offset {offset:x}"),
        }
    }

    fn read(&mut self, offset: u64, dst: &mut [u8]) {
        match offset {
            0x000 => (), // todo: read from stdin or something
            0x004 => dst.fill(0),
            0x018 => dst[0] = 0b010010000,
            0x008..0x014 | 0x01C | 0x04C..0x07C | 0x090..0xFCC => {
                panic!("reserved")
            }
            0x080..0x08C => panic!("reserved for test purposes"),
            0xFD0..0xFDC => panic!("reserved for future ID expansion"),
            0xFE0 => dst[0] = 0x11,
            0xFE4 => dst[0] = 0x10,
            0xFE8 => dst[0] = 0x04,
            0xFEC => dst[0] = 0x00,
            0xFF0 => dst[0] = 0x0D,
            0xFF4 => dst[0] = 0xF0,
            0xFF8 => dst[0] = 0x05,
            0xFFC => dst[0] = 0xB1,
            offset => unreachable!("got read on offset {offset:x}"),
        }
    }
}
