use {
    crate::{
        arch::x86::memory::PhysAddrExt,
        devices::pit::{self, PIT_FREQUENCY},
    },
    log::trace,
    spin::{Mutex, Once},
    x2apic::lapic::{LocalApicBuilder, TimerDivide, TimerMode, xapic_base},
    x86_64::PhysAddr,
};

pub static LAPIC: Once<Mutex<LocalApic>> = Once::INIT;

pub fn init() {
    LAPIC.call_once(|| Mutex::new(LocalApic::new()));
}

pub struct LocalApic {
    pub inner: x2apic::lapic::LocalApic,
    pub frequency: u32,
}

// :(
unsafe impl Send for LocalApic {}
unsafe impl Sync for LocalApic {}

impl LocalApic {
    pub fn new() -> Self {
        let base = PhysAddr::new(unsafe { xapic_base() }).to_virt();

        let mut lapic = LocalApicBuilder::new()
            .timer_vector(0x20)
            .error_vector(0xff)
            .spurious_vector(0xff)
            .set_xapic_base(base.as_u64())
            .build()
            .unwrap_or_else(|err| panic!("{}", err));

        unsafe {
            lapic.enable();
            lapic.disable_timer();
        }

        let frequency = calibrate_timer_frequency(&mut lapic);
        trace!("lapic frequency={}", frequency);

        Self {
            inner: lapic,
            frequency,
        }
    }

    pub fn start_periodic(&mut self, frequency: u32) {
        unsafe {
            self.inner.set_timer_mode(TimerMode::Periodic);
            self.inner.set_timer_divide(TimerDivide::Div16);
            self.inner
                .set_timer_initial((self.frequency >> 4) / frequency);
            self.inner.enable_timer();
        }
    }
}

fn calibrate_timer_frequency(lapic: &mut x2apic::lapic::LocalApic) -> u32 {
    unsafe {
        lapic.set_timer_initial(1);
        lapic.set_timer_mode(TimerMode::OneShot);
        lapic.set_timer_divide(TimerDivide::Div16);
        lapic.enable_timer();
    };

    let factor = 1000;
    let calibration_period = 10;
    let calibration_ticks = u16::try_from((PIT_FREQUENCY * calibration_period) / factor).unwrap();
    pit::init_oneshot(calibration_ticks);

    pit::start();
    unsafe { lapic.set_timer_initial(u32::MAX) };

    while !pit::is_expired() {
        unsafe { core::arch::asm!("") };
    }

    unsafe { lapic.disable_timer() };

    // Calculate the number of ticks per period (accounting for the LAPIC division)
    let ticks_per_period = (u32::MAX - unsafe { lapic.timer_current() }) << 4;

    let freq = (u64::from(ticks_per_period) * u64::from(factor)) / u64::from(calibration_period);

    trace!("ticks-per-period={ticks_per_period}, freq={freq}");

    u32::try_from(freq).unwrap()
}
