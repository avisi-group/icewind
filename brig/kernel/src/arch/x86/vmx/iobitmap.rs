use {
    crate::arch::x86::memory::{BoxToVirtAddrExt, VirtAddrExt},
    alloc::boxed::Box,
    bitset_core::BitSet,
    core::ops::{Deref, DerefMut},
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
