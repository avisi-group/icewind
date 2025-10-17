use {
    fastrand::Rng,
    spin::{Mutex, Once},
    uuid::Uuid,
    x86_64::instructions::random::RdRand,
};

static RNG: Once<Mutex<Rng>> = Once::INIT;

pub fn init() {
    RNG.call_once(|| {
        Mutex::new(Rng::with_seed(
            RdRand::new()
                .map(|rd| rd.get_u64())
                .flatten()
                .unwrap_or_else(|| {
                    log::warn!("no RDRAND, seeding with 0 instead");
                    0
                }),
        ))
    });
}

pub fn new_uuid_v4() -> Uuid {
    let mut buf = [0u8; 16];
    RNG.get().unwrap().lock().fill(&mut buf);
    uuid::Builder::from_random_bytes(buf).into_uuid()
}
