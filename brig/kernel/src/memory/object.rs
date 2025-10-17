use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::null_mut,
};

pub struct ObjectAllocator {
    cache16: SlabCache<16>,
    cache32: SlabCache<32>,
    cache64: SlabCache<64>,
    cache128: SlabCache<128>,
    cache256: SlabCache<256>,
    cache512: SlabCache<512>,
    cache1024: SlabCache<1024>,
    loa: LargeObjectAllocator,
}

unsafe impl GlobalAlloc for ObjectAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match layout.size() {
            ..16 => self.cache16.alloc(),
            ..32 => self.cache32.alloc(),
            ..64 => self.cache64.alloc(),
            ..128 => self.cache128.alloc(),
            ..256 => self.cache256.alloc(),
            ..512 => self.cache512.alloc(),
            ..1024 => self.cache1024.alloc(),
            _ => self.loa.alloc(layout),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.cache16.try_dealloc(ptr) {
            return;
        }

        if self.cache32.try_dealloc(ptr) {
            return;
        }

        if self.cache64.try_dealloc(ptr) {
            return;
        }

        if self.cache128.try_dealloc(ptr) {
            return;
        }

        if self.cache256.try_dealloc(ptr) {
            return;
        }

        if self.cache512.try_dealloc(ptr) {
            return;
        }

        if self.cache1024.try_dealloc(ptr) {
            return;
        }

        if self.loa.try_dealloc(ptr) {
            return;
        }

        panic!("unable to free object");
    }
}

struct SlabCache<const SIZE: usize, const PAGE_ORDER: usize>;

struct Slab<const SIZE: usize, const PAGE_ORDER: usize> {
    next: *mut Self,
    used_count: usize,
    used: usize,
}

impl<const S: usize, const P: usize> Slab<S, P> {
    pub fn new() -> Self {
        Self {
            next: null_mut(),
            used_count: 0,
            used: 0,
        }
    }

    pub fn allocate(&self) -> *mut u8 {
        //self.used.leading_ones()

        todo!()
    }
}

impl<const S: usize, const P: usize> SlabCache<S, P> {
    pub fn alloc(&self) -> *mut u8 {
        todo!()
    }

    pub fn try_dealloc(&self, ptr: *mut u8) -> bool {
        todo!()
    }
}

struct LargeObjectAllocator;

impl LargeObjectAllocator {
    pub fn alloc(&self, layout: Layout) -> *mut u8 {
        todo!()
    }

    pub fn try_dealloc(&self, ptr: *mut u8) -> bool {
        todo!()
    }
}
