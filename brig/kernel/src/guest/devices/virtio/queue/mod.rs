use {
    crate::{
        guest::devices::virtio::{
            devices::{Irq, Virtio},
            queue::defs::{BlkReq, BlkReqType},
        },
        host::{
            arch::x86::memory::{VirtAddrExt as _, VirtualMemoryArea, guest_physical_to_host_virt},
            devices::manager::SharedDeviceManager,
            objects::irq::IrqController,
        },
    },
    alloc::{alloc::alloc_zeroed, vec::Vec},
    core::{
        alloc::Layout,
        ptr::slice_from_raw_parts_mut,
        slice,
        sync::atomic::{AtomicBool, AtomicU32},
    },
    spin::Mutex,
    virtio_bindings::virtio_scsi::__u32,
    x86::fence,
    x86_64::{
        VirtAddr,
        structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB, Translate as _},
    },
};

const DESTINATION_DEVICE_BLOCK_SIZE: usize = 4096;

mod defs;
//mod descriptor;

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
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
struct VirtRingAvailHeader {
    flags: u16,
    index: u16,
}

#[repr(C, packed)]
struct VirtRingUsedElem {
    id: u32,
    len: u32,
}

#[repr(C, packed)]
struct VirtRingUsedHeader {
    flags: u16,
    idx: u16,
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
    lock: Mutex<()>,
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
            lock: Mutex::new(()),
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

    fn vring_descriptor(&mut self) -> *mut VirtRingDescr {
        self.descriptor_hva as *mut VirtRingDescr
    }

    fn available_header(&mut self) -> &mut VirtRingAvailHeader {
        let ptr = self.available_hva as *mut _;
        unsafe { &mut *ptr }
    }

    fn available_ring(&mut self) -> *mut u16 {
        let ptr = self.available_hva as *mut u16;
        unsafe { ptr.byte_add(size_of::<VirtRingAvailHeader>()) }
    }

    fn used_header(&mut self) -> &mut VirtRingUsedHeader {
        let ptr = self.used_hva as *mut _;
        unsafe { &mut *ptr }
    }

    fn used_ring(&mut self) -> *mut VirtRingUsedElem {
        let ptr = self.used_hva as *mut VirtRingUsedElem;
        unsafe { ptr.byte_add(size_of::<VirtRingUsedHeader>()) }
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

    fn push(&mut self, element_index: usize, size: u32) {
        // assert(elem_idx < _queue_num);

        // uint16_t idx = _used_descrs->idx % _queue_num;
        let idx = usize::from(self.used_header().idx) % self.queue_num;

        // _used_descrs->ring[idx].id = elem_idx;
        // _used_descrs->ring[idx].len = size;
        let descr = unsafe { &mut *self.used_ring().add(idx) };
        descr.id = u32::try_from(element_index).unwrap();
        descr.len = size;

        // assert(_used_descrs->flags == 0);
        assert!(self.used_header().flags == 0);

        // asm volatile("sfence" :: : "memory");
        fence::sfence();

        // __sync_fetch_and_add(&_used_descrs->idx, 1);
        // //_used_descrs->idx++;
        self.used_header().idx += 1;
    }

    fn pop(&mut self) -> Option<(usize, VirtRingDescr)> {
        // uint16_t num_heads = _avail_descrs->index - prev_idx;
        let num_heads = self.available_header().index - self.prev_idx;

        // assert(num_heads <= _queue_num);
        assert!(usize::from(num_heads) <= self.queue_num);

        // if (num_heads == 0) {
        // 	return NULL;
        // }
        if num_heads == 0 {
            return None;
        }

        // uint16_t head = _avail_descrs->ring[prev_idx++ % _queue_num];
        let head = unsafe {
            self.available_ring()
                .add(usize::from((self.prev_idx) + 1) % self.queue_num)
                .read()
        };

        self.prev_idx += 1;

        // assert(head < _queue_num);
        assert!(usize::from(head) < self.queue_num);

        // idx = head;

        // return get_descr(head);
        Some((usize::from(head), unsafe {
            self.vring_descriptor().add(usize::from(head)).read()
        }))
    }

    pub fn process(&mut self, irq: &Irq, isr: &AtomicU32) {
        log::debug!("processing queue");
        //  assert(queue);

        // 	DEBUG << CONTEXT(VirtIO) << "Processing queue " << queue;

        // 	queue->lock.lock();

        // 	while ((descr = queue->pop(idx)) != NULL) {
        // 		DEBUG << CONTEXT(VirtIO) << "Popped a descriptor chain head, idx="
        // << std::dec << idx;
        while let Some((idx, mut descr)) = self.pop() {
            log::debug!("popped: {idx}: {descr:?}");
            // 		VirtIOQueueEvent *evt = new VirtIOQueueEvent(queue, idx);
            let mut evt = VirtQueueEvent::new(idx);

            loop {
                log::debug!("start of loop: {descr:?}");
                // 			void *descr_host_addr;
                // 			if (!guest().resolve_gpa((gpa_t)descr->addr, descr_host_addr))
                // { 				ERROR << "Unable to resolve VirtIO descriptor
                // physical address to host address"; 				abort();
                // 			}
                let descr_host_addr = guest_physical_to_host_virt(descr.addr).as_mut_ptr();

                assert!(!descr.is_indirect());

                let buffer = VirtQueueEventBuffer {
                    data: descr_host_addr,
                    size: descr.length,
                };

                log::debug!("buffer: {buffer:?}");

                if descr.is_write() {
                    evt.write_buffers.push(buffer);
                } else {
                    evt.read_buffers.push(buffer);
                }

                if descr.has_next() {
                    descr = unsafe { self.vring_descriptor().add(usize::from(descr.next)).read() };
                } else {
                    break;
                }
            }

            // 		queue->lock.unlock();
            evt.process(self, irq, isr);
            // 		queue->lock.lock();

            // #ifdef SYNCHRONOUS
            // 		evt->complete.wait();
            // 		delete evt;
            // #endif
        }

        // 	queue->lock.unlock();
    }
}

struct VirtQueueEvent {
    complete: AtomicBool,
    read_buffers: Vec<VirtQueueEventBuffer>,
    write_buffers: Vec<VirtQueueEventBuffer>,
    response_size: u32,
    descriptor_index: usize,
}

impl VirtQueueEvent {
    fn new(descriptor_index: usize) -> Self {
        Self {
            complete: AtomicBool::new(false),
            read_buffers: Vec::new(),
            write_buffers: Vec::new(),
            response_size: 0,
            descriptor_index,
        }
    }

    fn process(&mut self, queue: &mut VirtQueue, irq: &Irq, isr: &AtomicU32) {
        log::debug!("processing event");

        let Some(first) = self.read_buffers.first() else {
            log::debug!("EMPTY EVENT?");
            return;
        };

        // Tom had this as >= but I think it's always =
        assert_eq!(usize::try_from(first.size).unwrap(), size_of::<BlkReq>());

        let req = unsafe { &*(first.data as *const BlkReq) };

        match req.typ {
            BlkReqType::In => self.handle_read_event(req.sector, queue, irq, isr),
            BlkReqType::Out => {
                // handle write event
            }
            BlkReqType::Flush => todo!(),
            BlkReqType::GetId => todo!(),
            BlkReqType::GetLifetime => todo!(),
            BlkReqType::Discard => todo!(),
            BlkReqType::WriteZeroes => todo!(),
            BlkReqType::SecureErase => todo!(),
        }
    }

    fn handle_read_event(
        &mut self,
        sector: u64,
        queue: &mut VirtQueue,
        irq: &Irq,
        isr: &AtomicU32,
    ) {
        log::debug!("read event: {sector:x}");
        assert_eq!(self.write_buffers.len(), 2);

        allocate_physical(self.write_buffers[0].data);

        let dev = SharedDeviceManager::get()
            .get_device_by_alias("disk00:04.0")
            .unwrap();
        let mut dev = dev.lock();
        let blk = dev.as_block();

        let destination = unsafe {
            core::slice::from_raw_parts_mut(
                self.write_buffers[0].data,
                usize::try_from(self.write_buffers[0].size).unwrap(),
            )
        };

        log::debug!("destination.len {}", destination.len());

        let offset = (usize::try_from(sector).unwrap() * 512) / blk.block_size();
        blk.read(destination, offset).unwrap();

        // callback logic just inlined here
        unsafe { self.write_buffers[1].data.write(0x00) }; // success

        self.response_size = 1 + self.write_buffers[0].size;
        self.submit(queue, irq, isr);
    }

    fn submit(&mut self, queue: &mut VirtQueue, irq: &Irq, isr: &AtomicU32) {
        queue.push(self.descriptor_index, self.response_size);

        let idx = 0;
        isr.fetch_or(1 << idx, core::sync::atomic::Ordering::Relaxed);
        // _irq.raise();
        log::debug!("pushed, raising irq");

        irq.controller.raise(irq.line);
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct VirtQueueEventBuffer {
    data: *mut u8,
    size: u32,
}

#[derive(Default, Debug, Clone, Copy)]
struct BlockDeviceRequest {
    block_offset: u64,
    block_count: u32,
    buffer: *mut u8,
    is_read: bool,
    opaque: *mut (),
}

/// If the supplied pointer does not have a physical mapping, allocate a new
/// backing page and map it
fn allocate_physical(ptr: *mut u8) {
    let address = VirtAddr::from_ptr(ptr);

    let physical = VirtualMemoryArea::current().opt.translate_addr(address);

    if physical.is_some() {
        return;
    }

    // Physical address lies within a RAM-backed region, so allocate a
    // backing page.
    let backing_page = VirtAddr::from_ptr(unsafe {
        alloc_zeroed(Layout::from_size_align(0x1000, 0x1000).unwrap())
    })
    .to_phys();

    // Map the allocated backing page into the 1-1 guest phyical memory area
    VirtualMemoryArea::current().map_page(
        Page::<Size4KiB>::from_start_address(address.align_down(0x1000u64)).unwrap(),
        PhysFrame::from_start_address(backing_page).unwrap(),
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
    );

    log::debug!(
        "allocated backing page {backing_page:x?} -> {:x?}",
        address.align_down(0x1000u64)
    );

    VirtualMemoryArea::current().map_page_propagate_invalidation(
        Page::<Size4KiB>::from_start_address(address.align_down(0x1000u64)).unwrap(),
        PhysFrame::from_start_address(backing_page).unwrap(),
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
    );
}
