use core::fmt;

pub struct UART16550Device(uart_16550::SerialPort);

impl UART16550Device {
    pub fn new(io_port: u16) -> Self {
        let mut serial_port = unsafe { uart_16550::SerialPort::new(io_port) };
        serial_port.init();
        Self(serial_port)
    }

    pub fn read_bytes(&mut self, buf: &mut [u8]) -> usize {
        let mut index = 0;

        while let Ok(byte) = self.0.try_receive() {
            buf[index] = byte;
            index += 1;
        }

        index
    }
}

impl fmt::Write for UART16550Device {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.write_str(s)
    }
}
