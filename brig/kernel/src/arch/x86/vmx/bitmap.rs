use {
    crate::arch::x86::memory::{BoxToVirtAddrExt, VirtAddrExt},
    alloc::boxed::Box,
    bitset_core::BitSet,
    core::ops::{Deref, DerefMut, Rem},
};

#[repr(align(4096))]
struct AlignedArray<const N: usize>([u8; N]);

impl<const N: usize> AlignedArray<N> {
    pub fn new() -> Box<Self> {
        Box::new(AlignedArray([0u8; _]))
    }
}

impl<const N: usize> Deref for AlignedArray<N> {
    type Target = [u8; N];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize> DerefMut for AlignedArray<N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct IoBitmap {
    region: Box<AlignedArray<8192>>,
}

impl IoBitmap {
    pub fn new() -> Self {
        let mut region = AlignedArray::new();
        region.fill(0xff);

        Self { region }
    }

    pub fn set_io_exiting(&mut self, port: u16, exiting: bool) {
        let byte = port / 8;
        let bit = port % 8;

        if exiting {
            self.region[usize::from(byte)].bit_set(bit as usize);
        } else {
            self.region[usize::from(byte)].bit_reset(bit as usize);
        }
    }

    pub fn get_a_phys(&self) -> u64 {
        self.region.as_virt().to_phys().as_u64()
    }

    pub fn get_b_phys(&self) -> u64 {
        self.get_a_phys() + 4096
    }
}

pub struct MsrBitmap {
    // four contiguous MSR bitmaps, which are each 1-KByte in size
    region: Box<AlignedArray<4096>>,
}

impl MsrBitmap {
    pub fn new() -> Self {
        let mut region = AlignedArray::new();

        region.fill(0x00);

        Self { region }
    }

    fn set_bit(&mut self, index: usize, value: bool) {
        let byte_index = index.div_floor(8);
        let bit_index = index.rem(8);

        let byte = &mut self.region[byte_index];

        if value {
            byte.bit_set(bit_index);
        } else {
            byte.bit_reset(bit_index);
        }
    }

    pub fn set_msr_write_exiting(&mut self, address: u32, exiting: bool) {
        let bit_index = usize::try_from(match address {
            0x0000_0000..=0x00001FFF => address + (2 * (8 * 1024)),
            0xC000_0000..=0xC0001FFF => (address - 0xC000_0000) + (3 * (8 * 1024)),
            _ => panic!("invalid MSR address {address:x}"),
        })
        .unwrap();

        self.set_bit(bit_index, exiting);
    }

    pub fn set_msr_read_exiting(&mut self, address: u32, exiting: bool) {
        let bit_index = usize::try_from(match address {
            0x0000_0000..=0x00001FFF => address,
            0xC000_0000..=0xC0001FFF => (address - 0xC000_0000) + 8 * 1024,
            _ => panic!("invalid MSR address {address:x}"),
        })
        .unwrap();

        self.set_bit(bit_index, exiting);
    }

    pub fn get_phys(&self) -> u64 {
        self.region.as_virt().to_phys().as_u64()
    }
}
