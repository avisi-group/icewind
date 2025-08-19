use {
    crate::host::objects::{
        Object, ObjectId, ObjectStore, ToIrqController, ToRegisterMappedDevice, ToTickable,
        device::{Device, MemoryMappedDevice},
        irq::IrqController,
    },
    alloc::{collections::BTreeMap, sync::Arc},
    common::intern::InternedString,
    core::sync::atomic::{AtomicU16, AtomicU32, Ordering},
    proc_macro_lib::guest_device_factory,
    spin::Mutex,
};

const IRQ_TXINTR: u32 = 1 << 5;
const IRQ_RXINTR: u32 = 1 << 4;

#[guest_device_factory(pl011)]
fn create_pl011(config: &BTreeMap<InternedString, InternedString>) -> Arc<dyn Device> {
    let dev = Arc::new(Pl011::new(
        66,
        *config
            .get(&InternedString::from_static("irq_controller"))
            .unwrap(),
    ));

    dev
}

struct Pl011 {
    id: ObjectId,
    control_register: AtomicU16,
    baud_rate: AtomicU16,
    fractional_baud: AtomicU16,
    line_control: AtomicU16,
    ifl: AtomicU16,
    irq_status: AtomicU32,
    irq_mask: AtomicU32,
    irq_line: usize,
    controller: Arc<dyn IrqController>,
}

impl Pl011 {
    fn new(irq_line: usize, controller_name: InternedString) -> Self {
        // Lookup GIC
        let gic_id = ObjectStore::global()
            .lookup_by_alias(controller_name)
            .unwrap();
        let controller = ObjectStore::global().get_irq_controller(gic_id).unwrap();

        Self {
            id: ObjectId::new(),

            baud_rate: AtomicU16::new(0),
            fractional_baud: AtomicU16::new(0),
            line_control: AtomicU16::new(0),
            control_register: AtomicU16::new(0x300),
            ifl: AtomicU16::new(0x12),

            irq_status: AtomicU32::new(0),
            irq_mask: AtomicU32::new(0),
            irq_line,
            controller,
        }
    }

    fn update_irq(&self) {
        if (self.irq_status.load(Ordering::Relaxed) & self.irq_mask.load(Ordering::Relaxed)) != 0 {
            self.controller.raise(self.irq_line);
        } else {
            self.controller.rescind(self.irq_line);
        }
    }
}

impl Object for Pl011 {
    fn id(&self) -> ObjectId {
        self.id
    }
}

// not tickable
impl ToTickable for Pl011 {}

// not a register mapped device
impl ToRegisterMappedDevice for Pl011 {}

// not an irq controller
impl ToIrqController for Pl011 {}

impl Device for Pl011 {
    fn start(&self) {}
    fn stop(&self) {}
}

impl MemoryMappedDevice for Pl011 {
    fn address_space_size(&self) -> u64 {
        0x1000
    }

    fn read(&self, offset: u64, dst: &mut [u8]) {
        match offset {
            // data register
            0x000 => (), // todo: read from stdin or something
            // receive status/error clear register
            0x004 => dst.fill(0),
            // flag register
            0x018 => {
                let mut data = 0;

                if (self.line_control.load(Ordering::Relaxed) & 0x10) == 0 {
                    data |= 1 << 6;
                }

                data |= 1 << 7;
                dst[0] = data;
            }
            0x024 => {
                dst[..2].copy_from_slice(
                    &self
                        .baud_rate
                        .load(core::sync::atomic::Ordering::Relaxed)
                        .to_le_bytes(),
                );
            }
            0x028 => {
                dst[..2].copy_from_slice(
                    &self
                        .fractional_baud
                        .load(core::sync::atomic::Ordering::Relaxed)
                        .to_le_bytes(),
                );
            }
            0x02c => {
                dst[..2].copy_from_slice(
                    &self
                        .line_control
                        .load(core::sync::atomic::Ordering::Relaxed)
                        .to_le_bytes(),
                );
            }

            0x030 => {
                dst[..2].copy_from_slice(
                    &self
                        .control_register
                        .load(core::sync::atomic::Ordering::Relaxed)
                        .to_le_bytes(),
                );
            }
            0x034 => {
                dst[..2].copy_from_slice(
                    &self
                        .ifl
                        .load(core::sync::atomic::Ordering::Relaxed)
                        .to_le_bytes(),
                );
            }
            0x038 => {
                dst[..2].copy_from_slice(
                    &(self.irq_mask.load(core::sync::atomic::Ordering::Relaxed) as u16)
                        .to_le_bytes(),
                );
            }
            0x03C => {
                dst[..4].copy_from_slice(
                    &self
                        .irq_status
                        .load(core::sync::atomic::Ordering::Relaxed)
                        .to_le_bytes(),
                );
            }
            0x008..0x014 | 0x01C | 0x04C..0x07C | 0x090..0xFCC => {
                panic!("reserved")
            }
            0x080..0x08C => panic!("reserved for test purposes"),
            0xFD0..0xFDC => panic!("reserved for future ID expansion"),
            0xFE0 => dst[0] = 0x11,
            0xFE4 => dst[0] = 0x10,
            0xFE8 => dst[0] = 0x14,
            0xFEC => dst[0] = 0x00,
            0xFF0 => dst[0] = 0x0D,
            0xFF4 => dst[0] = 0xF0,
            0xFF8 => dst[0] = 0x05,
            0xFFC => dst[0] = 0xB1,
            offset => panic!("got read on offset {offset:x}"),
        }
    }

    fn write(&self, offset: u64, src: &[u8]) {
        match offset {
            // data register
            0x000 => {
                crate::print!("{}", src[0] as char);
                self.irq_status
                    .fetch_or(IRQ_TXINTR, core::sync::atomic::Ordering::Relaxed);
                self.update_irq();
            }
            // receive status/error clear register
            0x004 => (),
            // flag register
            0x018 => panic!("read only register"),

            0x024 => {
                self.baud_rate.store(
                    u16::from_le_bytes(src.try_into().unwrap()) & 0xffff,
                    Ordering::Relaxed,
                );
            }
            0x028 => {
                self.fractional_baud.store(
                    u16::from_le_bytes(src.try_into().unwrap()) & 0x3f,
                    Ordering::Relaxed,
                );
            }
            0x02c => {
                self.line_control.store(
                    u16::from_le_bytes(src.try_into().unwrap()) & 0xff,
                    Ordering::Relaxed,
                );
            }

            0x030 => {
                self.control_register.store(
                    u16::from_le_bytes(src.try_into().unwrap()),
                    Ordering::Relaxed,
                );
            }

            0x034 => {
                self.ifl.store(
                    u16::from_le_bytes(src.try_into().unwrap()),
                    Ordering::Relaxed,
                );
                self.update_irq();
            }

            0x038 => {
                self.irq_mask.store(
                    u32::from(u16::from_le_bytes(src.try_into().unwrap()) & 0x7ff),
                    Ordering::Relaxed,
                );
                self.update_irq();
            }

            0x044 => {
                self.irq_status.fetch_and(
                    !u32::from(u16::from_le_bytes(src.try_into().unwrap())),
                    Ordering::Relaxed,
                );
                self.update_irq();
            }

            0x008..0x014 | 0x01C | 0x04C..0x07C | 0x090..0xFCC => {
                panic!("reserved")
            }
            0x080..0x08C => panic!("reserved for test purposes"),
            0xFD0..0xFDC => panic!("reserved for future ID expansion"),
            offset => panic!("got write on offset {offset:x}"),
        }
    }
}
