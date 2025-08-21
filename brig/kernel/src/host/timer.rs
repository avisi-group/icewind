use {
    crate::{
        host::{
            arch::x86::{MachineContext, irq::assign_irq},
            dbt::models::CURRENT_OPCODE,
            objects::tickable::Tickable,
        },
        println,
        scheduler::{self, TIMER_FREQUENCY},
        util::get_current_device,
    },
    alloc::{sync::Arc, vec::Vec},
    core::sync::atomic::{AtomicU64, Ordering},
    embedded_time::{
        duration::Nanoseconds,
        rate::{Hertz, Rate},
    },
    proc_macro_lib::irq_handler,
    spin::{Lazy, Mutex},
    x86::time::rdtscp,
};

const ENABLE_MEASUREMENTS: bool = true;

static TICKABLES: Lazy<Mutex<Vec<TickableState>>> = Lazy::new(|| Mutex::new(alloc::vec![]));

pub fn init() {
    assign_irq(0x20, timer_interrupt).unwrap();
}

pub static GLOBAL_CLOCK: GlobalClock = GlobalClock {
    nanoseconds_since_boot: AtomicU64::new(0),
};

pub struct GlobalClock {
    nanoseconds_since_boot: AtomicU64,
}

impl GlobalClock {
    pub fn increment(&self, amount: Nanoseconds<u64>) {
        self.nanoseconds_since_boot
            .fetch_add(amount.0 as u64, Ordering::Relaxed);
    }

    pub fn now(&self) -> Nanoseconds<u64> {
        Nanoseconds::<u64>::new(self.nanoseconds_since_boot.load(Ordering::Relaxed))
    }
}

#[irq_handler(with_code = false)]
fn timer_interrupt() {
    // Our hacked in timer frequency is 1000 Hz, a period of 1ms -> so, that's
    // 1,000,000 nanoseconds in a 1ms period
    GLOBAL_CLOCK.increment(Hertz::new(TIMER_FREQUENCY).to_duration().unwrap());

    let current_time = GLOBAL_CLOCK.now(); // TODO: compute this period from timer interrupt frequency

    handle_tickables(current_time);

    scheduler::schedule();

    unsafe {
        crate::host::devices::lapic::LAPIC
            .get()
            .unwrap()
            .lock()
            .inner
            .end_of_interrupt()
    };
}

pub struct Measurement {
    start: u64,
    prev: u64,
}

impl Measurement {
    pub fn start() -> Self {
        let now = if ENABLE_MEASUREMENTS {
            unsafe { rdtscp().0 }
        } else {
            0
        };

        Self {
            start: now,
            prev: now,
        }
    }

    pub fn trigger(&mut self, msg: &str) {
        if ENABLE_MEASUREMENTS {
            let now = unsafe { rdtscp().0 };
            let delta_start = now - self.start;
            let delta_prev = now - self.prev;

            println!(
                "MEASUREMENT: msg={} ds={}ms, dp={}ms",
                msg, delta_start, delta_prev
            );

            self.prev = unsafe { rdtscp().0 };
        }
    }
}

fn handle_tickables(current_time: Nanoseconds<u64>) {
    TICKABLES
        .lock()
        .iter_mut()
        .filter(|s| current_time >= s.time_at_last_tick + s.interval)
        .for_each(|s| {
            s.tickable.tick(current_time - s.time_at_last_tick);
            s.time_at_last_tick = current_time;
        });
}

pub fn register_tickable(interval: Nanoseconds<u64>, tickable: Arc<dyn Tickable>) {
    TICKABLES.lock().push(TickableState {
        tickable,
        interval,
        time_at_last_tick: Nanoseconds::new(0),
    });
}

struct TickableState {
    tickable: Arc<dyn Tickable>,
    interval: Nanoseconds<u64>,
    time_at_last_tick: Nanoseconds<u64>,
}
