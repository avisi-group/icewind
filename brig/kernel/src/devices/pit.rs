pub const PIT_FREQUENCY: u32 = 1193182;

const KEYBOARD_STATUS: u16 = 0x61;
const PIT_MODE_CMD: u16 = 0x43;
const PIT_CHANNEL_2: u16 = 0x42;

pub fn init_oneshot(period: u16) {
    unsafe {
        let data = x86::io::inb(KEYBOARD_STATUS);
        x86::io::outb(KEYBOARD_STATUS, data & 0xfc);

        x86::io::outb(PIT_MODE_CMD, 0xb2);

        x86::io::outb(PIT_CHANNEL_2, u8::try_from(period & 0xff).unwrap());
        x86::io::outb(PIT_CHANNEL_2, u8::try_from((period >> 8) & 0xff).unwrap());
    }
}

pub fn start() {
    unsafe {
        let data = x86::io::inb(KEYBOARD_STATUS);
        let masked = data & 0xfe;
        x86::io::outb(KEYBOARD_STATUS, masked);
        x86::io::outb(KEYBOARD_STATUS, masked | 1);
    }
}

pub fn is_expired() -> bool {
    unsafe { (x86::io::inb(KEYBOARD_STATUS) & 0x20) != 0 }
}
