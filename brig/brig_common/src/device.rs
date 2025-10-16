/// Emulated guest device
pub trait Device: Sync + Send {
    fn start(&self);
    fn stop(&self);
}

pub trait MemoryMappedDevice: Device {
    /// Size of the device's IO memory address space in bytes
    /// (I.E. the maximum valid sum of `offset` and `value.len()` in the `read`
    /// and `write` methods)
    fn address_space_size(&self) -> u64;

    /// Read `value.len()` bytes from the device starting at `offset`
    fn read(&self, offset: u64, value: &mut [u8]);

    /// Write `value` bytes into the device starting at `offset`
    fn write(&self, offset: u64, value: &[u8]);
}

pub trait RegisterMappedDevice: Device {
    /// Read `value.len()` bytes from the device at register `sys_reg_id`
    fn read(&self, sys_reg_id: u64, value: &mut [u8]);

    /// Write `value` bytes into the device at register `sys_reg_id`
    fn write(&self, sys_reg_id: u64, value: &[u8]);
}
