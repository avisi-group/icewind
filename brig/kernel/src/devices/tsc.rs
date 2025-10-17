use {
    crate::devices::pit::{self, PIT_FREQUENCY},
    log::trace,
    x86::time::rdtscp,
};

pub struct TSC;

impl TSC {
    pub fn calibrate() -> u32 {
        let factor = 1000;
        let calibration_period = 10;
        let calibration_ticks =
            u16::try_from((PIT_FREQUENCY * calibration_period) / factor).unwrap();
        pit::init_oneshot(calibration_ticks);

        let tsc_start = unsafe { rdtscp().0 };
        pit::start();

        while !pit::is_expired() {
            unsafe { core::arch::asm!("") };
        }

        let tsc_end = unsafe { rdtscp().0 };

        // Calculate the number of ticks per period (accounting for the LAPIC division)
        let ticks_per_period = tsc_end - tsc_start;

        let freq =
            (u64::from(ticks_per_period) * u64::from(factor)) / u64::from(calibration_period);

        trace!("ticks-per-period={ticks_per_period}, freq={freq}");

        u32::try_from(freq).unwrap()
    }
}
