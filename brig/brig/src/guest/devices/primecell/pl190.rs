use {
    crate::guest::devices::primecell::PRIMECELL_ID,
    alloc::sync::Arc,
    brig_common::device::{Device, MemoryMappedDevice},
    core::sync::atomic::{AtomicU8, AtomicU32, AtomicUsize, Ordering},
    kernel::host::objects::{ObjectId, irq::IrqController},
};

const PERIPHERAL_ID: u32 = 0x00041190;

pub struct Pl190 {
    id: ObjectId,
    irq_status: AtomicU32,
    soft_status: AtomicU32,
    mask: AtomicU32,
    fiq_select: AtomicU32,
    default_vector_address: AtomicU32,
    priority: AtomicUsize,
    prev_priority: [AtomicUsize; 17],
    prio_mask: [AtomicU32; 18],
    vector_addrs: [AtomicU32; 16],
    vector_controls: [AtomicU8; 16],
    irq_line: usize,
    fiq_line: usize,
    controller: Arc<dyn IrqController>,
}

impl Pl190 {
    pub fn new(irq_line: usize, fiq_line: usize, controller: Arc<dyn IrqController>) -> Self {
        let celf = Self {
            id: ObjectId::new(),
            irq_status: AtomicU32::new(0),
            soft_status: AtomicU32::new(0),
            mask: AtomicU32::new(0),
            fiq_select: AtomicU32::new(0),
            default_vector_address: AtomicU32::new(0),
            priority: AtomicUsize::new(17),
            prev_priority: Default::default(),
            prio_mask: Default::default(),
            vector_addrs: Default::default(),
            vector_controls: Default::default(),
            irq_line,
            fiq_line,
            controller,
        };

        celf.prio_mask[17].store(0xffff_ffff, Ordering::Relaxed);

        celf.update_vectors();

        celf
    }

    fn update_vectors(&self) {
        let mut mask = 0;

        for i in 0..16 {
            self.prio_mask[i].store(mask, Ordering::Relaxed);
            if (self.vector_controls[i].load(Ordering::Relaxed) & 0x20) != 0 {
                let n = self.vector_controls[i].load(Ordering::Relaxed) & 0x1f;
                mask |= 1 << n;
            }
        }

        self.prio_mask[16].store(mask, Ordering::Relaxed);
        self.update_lines();
    }

    fn update_lines(&self) {
        if (self.get_irq_status()
            & self.prio_mask[self.priority.load(Ordering::Relaxed)].load(Ordering::Relaxed))
            != 0
        {
            self.controller.raise(self.irq_line);
        } else {
            self.controller.rescind(self.irq_line);
        }

        if (self.get_fiq_status()
            & self.prio_mask[self.priority.load(Ordering::Relaxed)].load(Ordering::Relaxed))
            != 0
        {
            self.controller.raise(self.fiq_line);
        } else {
            self.controller.rescind(self.fiq_line);
        }
    }

    fn get_irq_status(&self) -> u32 {
        (self.irq_status.load(Ordering::Relaxed) | self.soft_status.load(Ordering::Relaxed))
            & self.mask.load(Ordering::Relaxed)
            & !self.fiq_select.load(Ordering::Relaxed)
    }

    fn get_fiq_status(&self) -> u32 {
        (self.irq_status.load(Ordering::Relaxed) | self.soft_status.load(Ordering::Relaxed))
            & self.mask.load(Ordering::Relaxed)
            & self.fiq_select.load(Ordering::Relaxed)
    }

    fn read_var(&self) -> u32 {
        let mut index = 0;
        for i in 0..self.priority.load(Ordering::Relaxed) {
            if ((self.irq_status.load(Ordering::Relaxed)
                | self.soft_status.load(Ordering::Relaxed))
                & self.prio_mask[i + 1].load(Ordering::Relaxed))
                != 0
            {
                index = i;
                break;
            }
        }

        if index == 17 {
            return self.default_vector_address.load(Ordering::Relaxed);
        }

        if index < self.priority.load(Ordering::Relaxed) {
            self.prev_priority[index]
                .store(self.priority.load(Ordering::Relaxed), Ordering::Relaxed);
            self.priority.store(index, Ordering::Relaxed);
            self.update_lines();
        }

        self.vector_addrs[self.priority.load(Ordering::Relaxed)].load(Ordering::Relaxed)
    }

    fn write_var(&self) {
        let priority = self.priority.load(Ordering::Relaxed);
        if priority < 17 {
            self.priority.store(
                self.prev_priority[priority].load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
        }
        self.update_lines();
    }
}

impl Device for Pl190 {
    fn start(&self) {}
    fn stop(&self) {}
}

impl MemoryMappedDevice for Pl190 {
    fn address_space_size(&self) -> u64 {
        0x1000
    }

    fn read(&self, offset: u64, value: &mut [u8]) {
        let response = match offset {
            0x000 => self.get_irq_status(),
            0x004 => self.get_fiq_status(),
            0x008 => {
                self.irq_status.load(Ordering::Relaxed) | self.soft_status.load(Ordering::Relaxed)
            }
            0x00c => self.fiq_select.load(Ordering::Relaxed),
            0x010 => self.mask.load(Ordering::Relaxed),
            0x018 => self.soft_status.load(Ordering::Relaxed),
            0x024 => 0,
            0x030 => self.read_var(),
            0x034 => self.default_vector_address.load(Ordering::Relaxed),
            0x100..=0x1ff => {
                todo!()
            }
            0x200..=0xfdf => {
                todo!()
            }
            0xfec => PERIPHERAL_ID >> 24,
            0xfe8 => PERIPHERAL_ID >> 16,
            0xfe4 => PERIPHERAL_ID >> 8,
            0xfe0 => PERIPHERAL_ID,
            0xffc => PRIMECELL_ID >> 24,
            0xff8 => PRIMECELL_ID >> 16,
            0xff4 => PRIMECELL_ID >> 8,
            0xff0 => PRIMECELL_ID,
            _ => panic!("unknown read offset {offset:x}"),
        };

        value.copy_from_slice(&response.to_le_bytes());
    }

    fn write(&self, offset: u64, value: &[u8]) {
        let data = u32::from_le_bytes(value.try_into().unwrap());

        match offset {
            0x000 | 0x004 | 0x008 => self.update_lines(),
            0x00c => {
                self.fiq_select.store(data, Ordering::Relaxed);
                self.update_lines();
            }
            0x010 => {
                self.mask.fetch_or(data, Ordering::Relaxed);
                self.update_lines();
            }

            0x014 => {
                self.mask.fetch_and(!data, Ordering::Relaxed);
                self.update_lines();
            }
            0x018 => {
                self.soft_status.fetch_or(data, Ordering::Relaxed);
                self.update_lines();
            }
            0x01c => {
                self.soft_status.fetch_and(!data, Ordering::Relaxed);
                self.update_lines();
            }
            0x030 => self.write_var(),
            0x034 => self.default_vector_address.store(data, Ordering::Relaxed),
            0x100..=0x1ff => {
                todo!()
            }
            _ => panic!("unknown write offset {offset:x}"),
        }
    }
}
