use {
    alloc::sync::Arc,
    bitfields::bitfield,
    common::{
        device::{Device, IrqController, RegisterMappedDevice, Tickable},
        sysreg_helpers::encode_sysreg_id,
    },
    core::sync::atomic::{AtomicBool, AtomicU64, Ordering},
    embedded_time::duration::Nanoseconds,
};

const CNTKCTL_EL1: u64 = encode_sysreg_id(3, 0, 14, 1, 0);

/// This register is provided so that software can discover the frequency of the
/// system counter.
const CNTFRQ_EL0: u64 = encode_sysreg_id(3, 3, 14, 0, 0);

const CNTPCT_EL0: u64 = encode_sysreg_id(3, 3, 14, 0, 1);
const CNTVCT_EL0: u64 = encode_sysreg_id(3, 3, 14, 0, 2);
const CNTP_TVAL_EL0: u64 = encode_sysreg_id(3, 3, 14, 2, 0);
const CNTP_CTL_EL0: u64 = encode_sysreg_id(3, 3, 14, 2, 1);
const CNTP_CVAL_EL0: u64 = encode_sysreg_id(3, 3, 14, 2, 2);
const CNTVOFF_EL2: u64 = encode_sysreg_id(3, 4, 14, 0, 3);
const CNTPS_TVAL_EL1: u64 = encode_sysreg_id(3, 7, 14, 2, 0);
const CNTPS_CTL_EL1: u64 = encode_sysreg_id(3, 7, 14, 2, 1);
const CNTPS_CVAL_EL1: u64 = encode_sysreg_id(3, 7, 14, 2, 2);
const CNTV_TVAL_EL0: u64 = encode_sysreg_id(3, 3, 14, 3, 0);
const CNTV_CTL_EL0: u64 = encode_sysreg_id(3, 3, 14, 3, 1);
const CNTV_CVAL_EL0: u64 = encode_sysreg_id(3, 3, 14, 3, 2);

pub struct GenericTimer {
    enabled: AtomicBool,

    irq_controller: Arc<dyn IrqController>,
    irq_line: usize,

    tick_interval: Nanoseconds<u64>,

    counter: AtomicU64,
    /// Used when registering for a periodic tick from the kernel
    frequency: AtomicU64,
    /// Subtracted from `counter` (physical counter) to produce the virtual
    /// count
    virtual_offset: AtomicU64,
    timer_condition_met: AtomicBool,
    timer_interrupt_masked: AtomicBool,
    timer_enabled: AtomicBool,
    compare_value: AtomicU64,

    cntkctl_el1: AtomicU64,
}

impl GenericTimer {
    pub fn new(
        controller: Arc<dyn IrqController>,
        irq_line: usize,
        tick_interval: Nanoseconds<u64>,
    ) -> Self {
        Self {
            enabled: AtomicBool::new(false),
            irq_line,
            irq_controller: controller,
            tick_interval,
            counter: AtomicU64::new(0),
            frequency: AtomicU64::new(10_000_000), //
            virtual_offset: AtomicU64::new(0),
            timer_condition_met: AtomicBool::new(false),
            timer_interrupt_masked: AtomicBool::new(false),
            timer_enabled: AtomicBool::new(false),
            compare_value: AtomicU64::new(0),
            cntkctl_el1: AtomicU64::new(0),
        }
    }

    fn physical_count(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    fn virtual_count(&self) -> u64 {
        self.counter.load(Ordering::Relaxed) - self.virtual_offset.load(Ordering::Relaxed)
    }

    pub fn tick_interval(&self) -> Nanoseconds<u64> {
        self.tick_interval
    }
}

impl Tickable for GenericTimer {
    fn tick(&self, time_since_last_tick: Nanoseconds<u64>) {
        if self.enabled.load(Ordering::Relaxed) {
            let counts =
                (time_since_last_tick.0 * self.frequency.load(Ordering::Relaxed)) / 1_000_000_000;

            self.counter.fetch_add(counts, Ordering::Relaxed);

            let interrupt_status = if self.timer_enabled.load(Ordering::Relaxed) {
                self.virtual_count() as i64 - self.compare_value.load(Ordering::Relaxed) as i64 >= 0
            } else {
                false
            };

            self.timer_condition_met
                .store(interrupt_status, Ordering::Relaxed);

            if interrupt_status && !self.timer_interrupt_masked.load(Ordering::Relaxed) {
                self.irq_controller.raise(self.irq_line);
            } else {
                self.irq_controller.rescind(self.irq_line);
            }
        }
    }
}

impl Device for GenericTimer {
    fn start(&self) {
        self.enabled.store(true, Ordering::Relaxed);
    }

    fn stop(&self) {
        self.enabled.store(false, Ordering::Relaxed);
    }
}

impl RegisterMappedDevice for GenericTimer {
    fn read(&self, sys_reg_id: u64, dest: &mut [u8]) {
        let value = match sys_reg_id {
            CNTKCTL_EL1 => self.cntkctl_el1.load(Ordering::Relaxed),
            CNTFRQ_EL0 => self.frequency.load(Ordering::Relaxed),
            CNTPCT_EL0 => todo!("CNTPCT_EL0"),
            CNTVCT_EL0 => self.counter.load(Ordering::Relaxed),
            CNTP_TVAL_EL0 => todo!("CNTP_TVAL_EL0"),
            CNTP_CTL_EL0 => todo!("CNTP_CTL_EL0"),
            CNTP_CVAL_EL0 => todo!("CNTP_CVAL_EL0"),
            CNTVOFF_EL2 => todo!("CNTVOFF_EL2"),
            CNTPS_TVAL_EL1 => todo!("CNTPS_TVAL_EL1"),
            CNTPS_CTL_EL1 => todo!("CNTPS_CTL_EL1"),
            CNTPS_CVAL_EL1 => todo!("CNTPS_CVAL_EL1"),
            CNTV_TVAL_EL0 => todo!("CNTV_TVAL_EL0"),
            CNTV_CTL_EL0 => {
                (self.timer_condition_met.load(Ordering::Relaxed) as u64) << 2
                    | (self.timer_interrupt_masked.load(Ordering::Relaxed) as u64) << 1
                    | (self.timer_enabled.load(Ordering::Relaxed) as u64)
            }
            CNTV_CVAL_EL0 => todo!("CNTV_CVAL_EL0"),
            _ => panic!("read unknown sys_reg_id {sys_reg_id:x}"),
        };

        //        get_host().print_message(&format!("read {sys_reg_id:x} = {value}\n"),
        // true);

        dest.copy_from_slice(&value.to_le_bytes());
    }

    fn write(&self, sys_reg_id: u64, value: &[u8]) {
        let value = u64::from_le_bytes(value.try_into().unwrap());

        //      get_host().print_message(&format!("write! {sys_reg_id:x} = {value}\n"),
        // true);

        match sys_reg_id {
            CNTKCTL_EL1 => self.cntkctl_el1.store(value, Ordering::Relaxed),
            CNTFRQ_EL0 => (), //self.frequency.store(value, Ordering::Relaxed),
            CNTPCT_EL0 => todo!("CNTPCT_EL0"),
            CNTVCT_EL0 => todo!("CNTVCT_EL0"),
            CNTP_TVAL_EL0 => todo!("CNTP_TVAL_EL0"),
            CNTP_CTL_EL0 => todo!("CNTP_CTL_EL0"),
            CNTP_CVAL_EL0 => todo!("CNTP_CVAL_EL0"),
            CNTVOFF_EL2 => self.virtual_offset.store(value, Ordering::Relaxed),
            CNTPS_TVAL_EL1 => todo!("CNTPS_TVAL_EL1"),
            CNTPS_CTL_EL1 => todo!("CNTPS_CTL_EL1"),
            CNTPS_CVAL_EL1 => todo!("CNTPS_CVAL_EL1"),
            CNTV_TVAL_EL0 => todo!("CNTV_TVAL_EL0"),
            CNTV_CTL_EL0 => {
                let enable = (value & 0b001) == 0b001;
                let imask = (value & 0b010) == 0b010;

                self.timer_enabled.store(enable, Ordering::Relaxed);
                self.timer_interrupt_masked.store(imask, Ordering::Relaxed);
            }
            CNTV_CVAL_EL0 => {
                self.compare_value.store(value, Ordering::Relaxed);
            }
            _ => panic!("write unknown sys_reg_id {sys_reg_id:x}"),
        }
    }
}

#[bitfield(u64)]
#[derive(Clone, Copy)]
struct CounterTimerKernelControlRegister {
    /// EL0 accesses to the frequency register and physical counter register are
    /// trapped.
    el0pcten: bool,
    /// EL0 accesses to the frequency register and virtual counter registers are
    /// trapped.
    el0vcten: bool,
    /// Enables the generation of an event stream from CNTVCT_EL0 as seen from
    /// EL1.
    evnten: bool,
    /// Controls which transition of the CNTVCT_EL0 trigger bit, as seen from
    /// EL1 and defined by EVNTI, generates an event when the event stream is
    /// enabled.
    ///
    /// EVNTDIR | Meaning
    /// -|-
    /// 0b0 | A 0 to 1 transition of the trigger bit triggers an event.
    /// 0b1 | A 1 to 0 transition of the trigger bit triggers an event.
    evntdir: bool,

    /// Selects which bit of CNTVCT_EL0, as seen from EL1, is the trigger for
    /// the event stream generated from that counter when that stream is
    /// enabled.
    #[bits(4)]
    evnti: u8,

    /// Traps EL0 accesses to the virtual timer registers to EL1, or to EL2 when
    /// it is implemented and enabled for the current Security state and
    /// HCR_EL2.TGE is 1
    el0vten: bool,

    /// Traps EL0 accesses to the physical timer registers to EL1, or to EL2
    /// when it is implemented and enabled for the current Security state and
    /// HCR_EL2.TGE is 1
    el0pten: bool,

    /// Reserved for software use in nested virtualization.
    el1pcten: bool,

    /// Reserved for software use in nested virtualization.
    el1pten: bool,

    /// Reserved for software use in nested virtualization.
    ecv: bool,

    /// Reserved for software use in nested virtualization.
    el1tvt: bool,

    /// Reserved for software use in nested virtualization.
    el1tvct: bool,

    /// Reserved for software use in nested virtualization.
    el1nvpct: bool,

    /// Reserved for software use in nested virtualization.
    el1nvvct: bool,

    /// Controls the scale of the generation of the event stream.
    ///
    /// ENVTIS | Meaning
    /// -|-
    /// 0b0	| The CNTKCTL_EL1.EVNTI field applies to CNTVCT_EL0[15:0].
    /// 0b1 | The CNTKCTL_EL1.EVNTI field applies to CNTVCT_EL0[23:8].
    evntis: bool,

    /// Reserved for software use in nested virtualization.
    cntvmask: bool,

    /// Reserved for software use in nested virtualization.
    cntpmask: bool,

    #[bits(44)]
    _reserved: u64,
}
