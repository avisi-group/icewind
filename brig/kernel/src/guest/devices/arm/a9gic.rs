use {
    crate::{
        guest::GuestExecutionContext,
        host::objects::{
            Object, ObjectId, ObjectStore, ToRegisterMappedDevice, ToTickable,
            device::{Device, MemoryMappedDevice},
            irq::IrqController,
        },
    },
    alloc::{collections::BTreeMap, sync::Arc, vec::Vec},
    common::intern::InternedString,
    core::{
        mem::MaybeUninit,
        sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        u8,
    },
    proc_macro_lib::guest_device_factory,
    spin::Mutex,
};

#[guest_device_factory(a9gic)]
fn create_gic(_config: &BTreeMap<InternedString, InternedString>) -> Arc<dyn Device> {
    Arc::new(GlobalInterruptController::new())
}

// Interrupt Controller Type Register
const GICD_TYPER: u32 = 0b100_000_00111;

// CPU Interface Identification Register
// 0x0002 = GICv2, 0x043B = Pretend to be ARM implementation (JEP106 code).
const GICC_IIDR: u32 = 0x0002_043B;

const PENDING_NONE: u32 = 0x0000_03ff;

const NO_LINE: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterruptId {
    PMUIRQ,
    COMMIRQ,
    CTIIRQ,
    COMMRX,
    COMMTX,
    CNTP,
    CNTHP,
    CNTHPS,
    CNTPS,
    CNTV,
    CNTHV,
    CNTHVS,
    PMBIRQ,
}

impl InterruptId {
    pub fn raw(&self) -> u32 {
        // SGI Interrupts are 0-15, PPI interrupts are 16-31, and SPI interrupts have an
        // offset of 32.
        const PPI_OFFSET: u32 = 16;

        match self {
            InterruptId::CNTP => 0x0000_000d + PPI_OFFSET,
            InterruptId::CNTHP => 0x0000_000a + PPI_OFFSET,
            InterruptId::CNTV => 0x0000_000b + PPI_OFFSET,
            _ => PENDING_NONE,
        }
    }
}

#[derive(Debug)]
struct GlobalInterruptController {
    id: ObjectId,
    lines: [IrqLine; 1020],
    distributor_enabled: AtomicBool,
    cpu_enabled: AtomicBool,
    cpu_pmr: AtomicU8,
    cpu_irq_line_pending: AtomicUsize,
    cpu_irq_line_running: AtomicUsize, // usize::MAX means None
}

#[derive(Debug)]
struct IrqLine {
    raised: AtomicBool,
    enabled: AtomicBool,
    active: AtomicBool,
    pending: AtomicBool,
    priority: AtomicU8,
    cpu_mask: AtomicU8,
    config: AtomicU8,
    last_active: AtomicUsize, // NO_LINE (usize::MAX) means None
}

impl IrqLine {
    pub fn new(config: u8, cpu_mask: u8) -> Self {
        Self {
            raised: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            active: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            priority: AtomicU8::new(0),
            cpu_mask: AtomicU8::new(cpu_mask),
            config: AtomicU8::new(config),
            last_active: AtomicUsize::new(NO_LINE),
        }
    }

    pub fn edge_triggered(&self) -> bool {
        (self.config.load(Ordering::Relaxed) & 2) == 2
    }

    pub fn level_triggered(&self) -> bool {
        (self.config.load(Ordering::Relaxed) & 2) == 0
    }
}

impl GlobalInterruptController {
    fn new() -> Self {
        let lines = core::array::from_fn(|i| {
            let mut config = 0u8;
            let mut cpu_mask = 0u8;

            if i < 16 {
                config |= 2u8;
            }

            if i < 32 {
                cpu_mask = 1u8;
            }

            IrqLine::new(config, cpu_mask)
        });

        Self {
            id: ObjectId::new(),
            lines,
            distributor_enabled: AtomicBool::new(false),
            cpu_enabled: AtomicBool::new(false),
            cpu_pmr: AtomicU8::new(0),
            cpu_irq_line_pending: AtomicUsize::new(NO_LINE), // NO_LINE (usize::MAX) means None
            cpu_irq_line_running: AtomicUsize::new(NO_LINE),
        }
    }

    fn lines_for_bitvector(&self, base: u64, len: u64, bits: u64) -> &[IrqLine] {
        let start_index = (8 * base) / bits;
        let end_index = core::cmp::min(((8 * len) / bits) + start_index, 1019);

        &self.lines[start_index as usize..end_index as usize]
    }

    fn acknowledge(&self) -> u32 {
        log::debug!("--- CPU ACKNOWLEDGE ---");

        // src/captive/src/devices/arm/gic.cpp:354
        let current_pending_index = self.cpu_irq_line_pending.load(Ordering::Relaxed);

        if current_pending_index == NO_LINE {
            self.cpu_irq_line_running.store(NO_LINE, Ordering::Relaxed);

            log::debug!("no current pending");
            log::debug!("--- CPU ACK DONE ---");
            return 1023;
        }

        let current_pending = &self.lines[current_pending_index];
        if u32::from(current_pending.priority.load(Ordering::Relaxed)) >= self.running_priority() {
            log::debug!("current pending priority is lower than running priority");
            log::debug!("--- CPU ACK DONE 1023 ---");

            return 1023;
        }

        log::debug!("updating last active");
        // irq->_last_active = _current_running;
        current_pending.last_active.store(
            self.cpu_irq_line_running.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );

        log::debug!("un-pending current pending");
        // irq->_pending = false;
        current_pending.pending.store(false, Ordering::Relaxed);
        // _current_running = irq;

        log::debug!("setting current running to {current_pending_index}");
        self.cpu_irq_line_running
            .store(current_pending_index, Ordering::Relaxed);

        self.update();

        log::debug!("--- CPU ACK DONE {current_pending_index} ---");

        return u32::try_from(current_pending_index).unwrap();
    }

    fn update(&self) {
        log::debug!("--- CPU UPDATE ---");
        //     //fprintf(stderr, "***** UPDATE\n");

        // _current_pending = nullptr;

        self.cpu_irq_line_pending.store(NO_LINE, Ordering::Relaxed);

        // if (!_enabled || !_gic._distributor._enabled) {
        // 	//fprintf(stderr, "rescind\n");
        // 	_irq.rescind();
        // 	return;
        // }
        if !self.cpu_enabled.load(Ordering::Relaxed)
            || !self.distributor_enabled.load(Ordering::Relaxed)
        {
            log::debug!("Distributor or CPU not enabled");
            cpu_irq_rescind();
            log::debug!("--- CPU UPDATE DONE ---");
            return;
        }

        // GICIRQLine *best_irq = nullptr;
        // for (const auto line : _gic._irq_lines) {

        let mut best_irq_idx = None;

        for (i, line) in self.lines.iter().enumerate() {
            if line.enabled.load(Ordering::Relaxed)
                && (line.pending.load(Ordering::Relaxed)
                    || (line.level_triggered() && line.raised.load(Ordering::Relaxed)))
            {
                log::debug!("line {i} that is enabled, and pending, or level-triggered raised");

                best_irq_idx = if let Some(best_irq_idx) = best_irq_idx {
                    let best_irq: &IrqLine = &self.lines[best_irq_idx];
                    if line.priority.load(Ordering::Relaxed)
                        < best_irq.priority.load(Ordering::Relaxed)
                    {
                        log::debug!("we're better than existing best irq {best_irq_idx}");
                        Some(i)
                    } else {
                        log::debug!("we're not better than existing best irq {best_irq_idx}");
                        Some(best_irq_idx) // no change
                    }
                } else {
                    log::debug!("no existing best irq -- updating to this");
                    Some(i)
                };
            }
        }
        // 	if (line->_enabled && (line->_pending || (line->level_triggered() &&
        // line->raised()))) {  if (!best_irq || line->_priority <
        // best_irq->_priority) { 			best_irq = line;
        // 		}
        // 	}
        // }

        // bool raise = false;
        let mut raise = false;

        // if (best_irq) {
        if let Some(best_irq_idx) = best_irq_idx {
            log::debug!("we have a best irq {best_irq_idx}");

            let best_irq_priority = self.lines[best_irq_idx].priority.load(Ordering::Relaxed);
            log::debug!("best irq priority is: {best_irq_priority}");
            // 	if (best_irq->_priority < _pmr) {
            if best_irq_priority < self.cpu_pmr.load(Ordering::Relaxed) {
                log::debug!("priority is better than pmr, so setting current pending");

                // 		_current_pending = best_irq;
                self.cpu_irq_line_pending
                    .store(best_irq_idx, Ordering::Relaxed);

                // 		if (best_irq->_priority < running_priority()) {
                // 			//fprintf(stderr, "***** yoyoyo %p %u\n", best_irq,
                // best_irq->index()); 			raise = true;
                // 		}
                if u32::from(best_irq_priority) < self.running_priority() {
                    log::debug!("priority is higher than running priority, raising irq");
                    raise = true;
                }
            }
        }

        if raise {
            // 	//fprintf(stderr, "raise\n");
            // 	_irq.raise();
            cpu_irq_raise();
        } else {
            // 	//fprintf(stderr, "rescind\n");
            // _irq.rescind();
            cpu_irq_rescind();
        }

        log::debug!("--- CPU UPDATE DONE ---");
    }

    fn running_priority(&self) -> u32 {
        let line = self.cpu_irq_line_running.load(Ordering::Relaxed);

        if line == NO_LINE {
            0x100
        } else {
            u32::from(self.lines[line].priority.load(Ordering::Relaxed))
        }
    }

    fn eoi(&self, irqid: usize) {
        log::debug!("--- CPU EOI line={irqid} ---");

        // if (irqid >= _gic._irq_lines.size()) {
        //  return;
        // }
        if (irqid >= self.lines.len()) {
            log::debug!("invalid line");
            log::debug!("--- CPU EOI DONE ---");
            return;
        }

        // if (_current_running == nullptr) {
        // 	return;
        // }
        let current_running_idx = self.cpu_irq_line_running.load(Ordering::Relaxed);
        if (current_running_idx == NO_LINE) {
            log::debug!("nothing running");
            log::debug!("--- CPU EOI DONE ---");
            return;
        }

        let current_running = &self.lines[current_running_idx];

        log::debug!("current running = {current_running:?}");

        // GICIRQLine *irq = &_gic.get_irq_line(irqid);
        let irq = &self.lines[irqid];

        // if (irq != _current_running) {
        if irqid != current_running_idx {
            log::debug!("asked to eoi not current irq");

            // GICIRQLine *last = _current_running;
            let mut last_idx = current_running_idx;

            log::debug!("last idx = {last_idx}");

            // while (last) {
            // 		if (last == irq) {
            // 			last->_last_active = irq->_last_active;
            // 			break;
            // 		}

            // 		last = last->_last_active;
            // }

            while (last_idx != NO_LINE) {
                if (last_idx == irqid) {
                    log::debug!("found ourselves - removing from chain");

                    self.lines[last_idx]
                        .last_active
                        .store(irq.last_active.load(Ordering::Relaxed), Ordering::Relaxed);
                    break;
                }

                last_idx = self.lines[last_idx].last_active.load(Ordering::Relaxed);
                log::debug!("next last idx = {last_idx}");
            }
        } else {
            // else {
            // 	_current_running = _current_running->_last_active;
            // }
            log::debug!("we are running, so prepend us to chain");

            self.cpu_irq_line_running.store(
                current_running.last_active.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
        }

        //fprintf(stderr, "  cr=%p\n", _current_running);
        log::debug!(
            "cpu_irq_line_running: {:x}",
            self.cpu_irq_line_running.load(Ordering::Relaxed)
        );

        self.update();
        log::debug!("--- CPU EOI DONE ---");
    }
}

impl Object for GlobalInterruptController {
    fn id(&self) -> ObjectId {
        self.id
    }
}

impl Device for GlobalInterruptController {
    fn start(&self) {}
    fn stop(&self) {}
}

impl ToTickable for GlobalInterruptController {}
impl ToRegisterMappedDevice for GlobalInterruptController {}

impl MemoryMappedDevice for GlobalInterruptController {
    fn address_space_size(&self) -> u64 {
        0x3000
    }

    /// Read `value.len()` bytes from the device starting at `offset`
    fn read(&self, offset: u64, value: &mut [u8]) {
        log::debug!("read GIC @ {offset:x}");

        let response = match offset {
            0x1000 => {
                // GICD_CTLR
                self.distributor_enabled.load(Ordering::Relaxed) as u32
            }
            0x1004 => 0x81f,
            0x1008 => 0x200143b,
            0x1100..=0x117c | 0x1300..=0x137c => 0,
            0x1800..=0x1bfb => {
                // GICD_ITARGETS
                self.lines_for_bitvector(offset - 0x1800, 4, 8)
                    .iter()
                    .enumerate()
                    .fold(0, |acc, (index, line)| {
                        acc | ((line.cpu_mask.load(Ordering::Relaxed) as u32) << (index * 8))
                    })
            }
            0x1c00..=0x1dff => {
                // GICD_ICFG
                self.lines_for_bitvector(offset - 0x1c00, 4, 2)
                    .iter()
                    .enumerate()
                    .fold(0u32, |acc, (index, line)| {
                        acc | ((line.config.load(Ordering::Relaxed) as u32) << (index * 2)) as u32
                    })
            }
            0x2000 => {
                // GICC_CTLR
                self.cpu_enabled.load(Ordering::Relaxed) as u32
            }
            0x200c => {
                // GICC_IAR
                self.acknowledge()
            }
            0x20fc => 0,
            _ => {
                panic!("[GIC] Unhandled read offset: {offset:x}");
                // 0u32
            }
        };

        log::debug!("read GIC @ {offset:x}, got {response:x}");

        value.copy_from_slice(&response.to_le_bytes());
    }

    /// Write `value` bytes into the device starting at `offset`
    fn write(&self, offset: u64, value: &[u8]) {
        let value = match *value {
            [x] => u32::from(x),
            [a, b, c, d] => u32::from_ne_bytes([a, b, c, d]),
            _ => todo!("{offset:x} <= {value:?}"),
        };

        log::debug!("write GIC @ {offset:x} = {value:x}");

        match offset {
            0x1000 => {
                self.distributor_enabled
                    .store((value & 1) == 1, Ordering::Relaxed);
            }
            0x1100..=0x117f => {
                // GICD_ISENABLE
                self.lines_for_bitvector(offset - 0x1100, 4, 1)
                    .iter()
                    .enumerate()
                    .for_each(|(index, line)| {
                        let enable = ((value >> index) & 1) == 1;
                        line.enabled.fetch_or(enable, Ordering::Relaxed);
                    })
            }
            0x1180..=0x11ff => {
                // GICD_ICENABLE
                self.lines_for_bitvector(offset - 0x1180, 4, 1)
                    .iter()
                    .enumerate()
                    .for_each(|(index, line)| {
                        let clear_enable = ((value >> index) & 1) == 1;
                        line.enabled.fetch_nand(clear_enable, Ordering::Relaxed);
                    })
            }
            0x1380..=0x13ff => {
                // GICD_ICACTIVE
                self.lines_for_bitvector(offset - 0x1380, 4, 1)
                    .iter()
                    .enumerate()
                    .for_each(|(index, line)| {
                        let clear_active = ((value >> index) & 1) == 1;
                        line.active.fetch_nand(clear_active, Ordering::Relaxed);
                    })
            }
            0x1400..=0x17fb => {
                // GICD_IPRIORITY
                self.lines_for_bitvector(offset - 0x1400, 4, 8)
                    .iter()
                    .enumerate()
                    .for_each(|(index, line)| {
                        line.priority
                            .store(((value >> (index * 8)) & 0xff) as u8, Ordering::Relaxed)
                    })
            }
            0x1800..=0x1bfb => {
                // GICD_ITARGETS
                self.lines_for_bitvector(offset - 0x1800, 4, 8)
                    .iter()
                    .enumerate()
                    .for_each(|(index, line)| {
                        line.cpu_mask
                            .store(((value >> (index * 8)) & 0xff) as u8, Ordering::Relaxed)
                    })
            }
            0x1c00..=0x1dff => {
                // GICD_ICFG
                self.lines_for_bitvector(offset - 0x1c00, 4, 2)
                    .iter()
                    .enumerate()
                    .for_each(|(index, line)| {
                        let cfg_val = ((value >> (index * 2)) & 0x3) as u8;

                        log::debug!("updating config for line offset={} to {:x}", index, cfg_val);

                        line.config.store(cfg_val, Ordering::Relaxed)
                    })
            }
            0x1f00 => {
                todo!("sgir");
            }
            0x1f10..=0x1f1f => {
                // GICD_CPENDSGIR
                todo!("cpendsgr")
            }
            0x1f20..=0x1f2f => {
                // GICD_SPENDSGIR
                todo!("spendsgr")
            }
            // GICC
            0x2000 => {
                // GICC_CTLR
                self.cpu_enabled.store((value & 1) == 1, Ordering::Relaxed);
            }
            0x2004 => {
                // GICC_PMR
                self.cpu_pmr.store((value & 0xff) as u8, Ordering::Relaxed);
            }
            0x2010 => {
                // GICC_EOIR
                self.eoi(usize::try_from(value).unwrap());
            }
            _ => {
                panic!("[GIC] Write offset: {offset:x} <= {value:x}");
            }
        }
    }
}

impl IrqController for GlobalInterruptController {
    fn raise(&self, line: usize) {
        let x = line;
        let line = &self.lines[line];

        if let Ok(false) =
            line.raised
                .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        {
            log::debug!("[GIC] raise irq {x}");

            if line.edge_triggered() {
                line.pending.store(true, Ordering::Relaxed);
            }

            self.update();
        }
    }

    fn rescind(&self, line: usize) {
        let x = line;
        let line = &self.lines[line];

        if let Ok(true) =
            line.raised
                .compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed)
        {
            log::debug!("[GIC] rescind irq {x}");
            self.update();
        }
    }
}

fn cpu_irq_raise() {
    log::debug!("cpu irq raise");
    GuestExecutionContext::current()
        .interrupt_pending
        .store(1, Ordering::Relaxed);
}

fn cpu_irq_rescind() {
    log::debug!("cpu irq rescind");
    GuestExecutionContext::current()
        .interrupt_pending
        .store(0, Ordering::Relaxed);
}
