use {
    alloc::boxed::Box,
    core::{
        alloc::{AllocError, Allocator, Layout},
        cell::RefCell,
        fmt::{self, Debug},
        ptr::NonNull,
        sync::atomic::{AtomicUsize, Ordering},
    },
};

pub struct BumpAllocator {
    data: RefCell<Box<[u8]>>,
    position: AtomicUsize,
}

impl Debug for BumpAllocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BumpAllocator")
            .field("data_len", &self.data.borrow().len())
            .field("position", &self.position)
            .finish()
    }
}

impl BumpAllocator {
    pub fn new(len: usize) -> Self {
        Self {
            data: RefCell::new(unsafe {
                Box::new_uninit_slice(len)
                    // safe because all possible byte values for a `u8` is a valid `u8`
                    .assume_init()
            }),
            position: AtomicUsize::new(0),
        }
    }

    pub fn clear(&mut self) {
        self.position.store(0, Ordering::Relaxed);
    }
}

unsafe impl Allocator for BumpAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let position = self.position.load(Ordering::Relaxed);

        // aligned position for this allocation
        let aligned = position + (layout.align() - (position % layout.align()));

        let next_position = aligned + layout.size();

        if next_position >= self.data.borrow().len() {
            // insufficient memory for the current allocation
            return Err(AllocError);
        }

        self.position.store(next_position, Ordering::Relaxed);

        Ok(unsafe {
            NonNull::new_unchecked(
                &mut self.data.borrow_mut()[aligned..aligned + layout.size()] as *mut _,
            )
        })
    }

    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {
        // no-op
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BumpAllocatorRef<'a>(&'a BumpAllocator);

impl<'a> BumpAllocatorRef<'a> {
    pub fn new(allocator: &'a BumpAllocator) -> Self {
        Self(allocator)
    }
}

unsafe impl<'a> Allocator for BumpAllocatorRef<'a> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.0.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { self.0.deallocate(ptr, layout) }
    }
}
