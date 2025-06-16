use crate::host::arch::x86::memory::guest_physical_to_host_virt;

mod defs;
//mod descriptor;

struct VirtRingDescr {
    addr: u64,
    length: u32,
    flags: u16,
    next: u16,
}

impl VirtRingDescr {
    fn has_next(&self) -> bool {
        (self.flags & 1) == 1
    }

    fn is_write(&self) -> bool {
        (self.flags & 2) == 2
    }

    fn is_indirect(&self) -> bool {
        (self.flags & 4) == 4
    }
}

#[repr(C, packed)]
struct VirtRingAvail<T> {
    flags: u16,
    index: u16,
    ring: T, // [u16]
}

#[repr(C, packed)]
struct VirtRingUsedElem {
    id: u32,
    len: u32,
}

#[repr(C, packed)]
struct VirtRingUsed<T> {
    flags: u16,
    idx: u16,
    ring: T, //[VirtRingUsedElem],
}

#[derive(Debug)]
pub struct VirtQueue {
    index: usize,
    ready: bool,
    queue_num: usize,
    descriptor_gpa: u64,
    available_gpa: u64,
    used_gpa: u64,
    descriptor_hva: u64,
    available_hva: u64,
    used_hva: u64,
    prev_idx: u16,
}

impl VirtQueue {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            ready: false,
            queue_num: 0,
            descriptor_gpa: 0,
            available_gpa: 0,
            used_gpa: 0,
            descriptor_hva: 0,
            available_hva: 0,
            used_hva: 0,
            prev_idx: 0,
        }
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    pub fn set_ready(&mut self, ready: bool) {
        self.ready = ready;
        if ready {
            self.update_host_addresses();
        }
    }
    pub fn num_max(&self) -> usize {
        0x1000
    }

    pub fn set_num(&mut self, num: usize) {
        self.queue_num = num;
    }

    pub fn num(&self) -> usize {
        self.queue_num
    }

    fn update_host_addresses(&mut self) {
        self.descriptor_hva = guest_physical_to_host_virt(self.descriptor_gpa).as_u64();
        self.available_hva = guest_physical_to_host_virt(self.available_gpa).as_u64();
        self.used_hva = guest_physical_to_host_virt(self.used_gpa).as_u64();

        self.init_vring();
    }

    fn init_vring(&mut self) {
        self.prev_idx = 0;
        // _vring_descrs = (VirtRingDescr *) _descriptor_hva;
        // 				_avail_descrs = (VirtRingAvail *) _avail_hva;
        // 				_used_descrs = (VirtRingUsed *) _used_hva;

        // 				prev_idx = 0;
    }

    pub fn set_descriptor_low(&mut self, value: u32) {
        self.descriptor_gpa &= 0xffff_ffff_0000_0000;
        self.descriptor_gpa |= u64::from(value);
    }
    pub fn set_descriptor_high(&mut self, value: u32) {
        self.descriptor_gpa &= 0x0000_0000_ffff_ffff;
        self.descriptor_gpa |= u64::from(value) << 32;
    }

    pub fn set_available_low(&mut self, value: u32) {
        self.available_gpa &= 0xffff_ffff_0000_0000;
        self.available_gpa |= u64::from(value);
    }
    pub fn set_available_high(&mut self, value: u32) {
        self.available_gpa &= 0x0000_0000_ffff_ffff;
        self.available_gpa |= u64::from(value) << 32;
    }
    pub fn set_used_low(&mut self, value: u32) {
        self.used_gpa &= 0xffff_ffff_0000_0000;
        self.used_gpa |= u64::from(value);
    }
    pub fn set_used_high(&mut self, value: u32) {
        self.used_gpa &= 0x0000_0000_ffff_ffff;
        self.used_gpa |= u64::from(value) << 32;
    }
}
