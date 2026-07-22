use {
    crate::ptwrite::reader::{BUFFER_SIZE, READY},
    memmap2::MmapRaw,
    perf_event_open_sys::bindings::perf_event_mmap_page,
    std::{
        ptr,
        sync::atomic::{Ordering, fence},
    },
};

pub struct RingBufferAux {
    mmap: MmapRaw,
    aux_area: MmapRaw,
}

impl RingBufferAux {
    pub fn new(mmap: MmapRaw, aux_area: MmapRaw) -> Self {
        Self { mmap, aux_area }
    }

    fn page(&self) -> *mut perf_event_mmap_page {
        self.mmap.as_mut_ptr() as *mut _
    }

    pub fn next_data<F: FnOnce(&[u8], Option<&[u8]>) -> usize>(&mut self, callback: F) -> bool {
        let page = self.page();

        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page.
        // - aux_tail is only written by the user side so it is safe to do a non-atomic
        //   read here.
        let tail = unsafe { read_tail(page) };

        // ATOMICS:
        // - The acquire load here syncronizes with the release store in the kernel and
        //   ensures that all the data written to the ring buffer before aux_head is
        //   visible to this thread.
        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page.
        let head = unsafe { read_head(page) };

        let len = head - tail;

        if len < BUFFER_SIZE / 2 {
            READY.store(true, Ordering::Relaxed);
        }
        if len > ((BUFFER_SIZE * 100) / 90) {
            panic!(
                "Ring buffer exceeded >90% capacity: head: {head:x}, tail: {tail:x}, len: {len:x}, capacity: {BUFFER_SIZE:x}",
            );
        }

        // head and tail constantly increase, need to wrap them to index the ring buffer
        let wrapped_head = head % BUFFER_SIZE;
        let wrapped_tail = tail % BUFFER_SIZE;

        let (main, secondary) = if wrapped_head > wrapped_tail {
            (
                unsafe {
                    std::slice::from_raw_parts(
                        self.aux_area.as_ptr().add(wrapped_tail),
                        wrapped_head - wrapped_tail,
                    )
                },
                None,
            )
        } else {
            let a = unsafe {
                std::slice::from_raw_parts(
                    self.aux_area.as_ptr().add(wrapped_tail),
                    BUFFER_SIZE - wrapped_tail,
                )
            };
            let b = unsafe { std::slice::from_raw_parts(self.aux_area.as_ptr(), wrapped_head) };
            (a, Some(b))
        };

        let consumed = callback(main, secondary);

        // ATOMICS:
        // - The release store here prevents the compiler from re-ordering any reads
        //   past the store to aux_tail.
        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page
        unsafe {
            write_tail(page, tail + consumed);
        }

        consumed > 0
    }
}

unsafe fn read_head(page: *const perf_event_mmap_page) -> usize {
    let page = &*page;
    let head = ptr::read_volatile(&page.aux_head);
    fence(Ordering::Acquire);
    head as usize
}

unsafe fn read_tail(page: *const perf_event_mmap_page) -> usize {
    let page = &*page;
    // No memory fence required because we're just reading a value previously
    // written by us.
    ptr::read_volatile(&page.aux_tail) as usize
}

unsafe fn write_tail(page: *mut perf_event_mmap_page, value: usize) {
    let page = &mut *page;
    fence(Ordering::AcqRel);
    ptr::write_volatile(&mut page.aux_tail, value as u64);
}

pub enum Buffer<'a> {
    Single(&'a [u8]),
    Split(&'a [u8], &'a [u8]),
}
