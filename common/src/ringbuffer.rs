use core::{
    any::type_name,
    cmp::{Ordering, min},
    fmt::Debug,
    marker::PhantomData,
    mem::offset_of,
};

pub trait Role {}

pub struct Producer;
impl Role for Producer {}

pub struct Consumer;
impl Role for Consumer {}

/// Shared slice ringbuffer
pub struct RingBuffer<'a, T: Role> {
    header: &'a mut [u8],
    buffer: &'a mut [u8],

    _kind: PhantomData<T>,
}

impl<'a, T: Role> Debug for RingBuffer<'a, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "RingBuffer<{}> {{ capacity: {:#x}, head: {:#x}, tail: {:#x} }}",
            type_name::<T>(),
            self.capacity(),
            self.head(),
            self.tail()
        )
    }
}

impl<'a, T: Role> RingBuffer<'a, T> {
    /// Initialize a new ringbuffer
    pub fn init(mem: &'a mut [u8]) -> Self {
        let mut celf = Self::open(mem);

        let len = celf.buffer.len();
        celf.set_capacity(len);
        celf.set_head(0);
        celf.set_tail(0);

        celf
    }

    /// Open an already-initialized ringbuffer
    pub fn open(mem: &'a mut [u8]) -> Self {
        let (header, buffer) = mem.split_at_mut(size_of::<Header>());

        let celf = Self {
            header,
            buffer,
            _kind: PhantomData::default(),
        };

        // breaks init because `open` is re-used there
        // assert!(celf.capacity() > 0);

        celf
    }

    fn read_header_field(&self, offset: usize) -> usize {
        usize::try_from(unsafe { (self.header.as_ptr().add(offset) as *const u64).read_volatile() })
            .unwrap()
    }

    fn write_header_field(&mut self, offset: usize, value: usize) {
        let value = u64::try_from(value).unwrap();
        unsafe { (self.header.as_mut_ptr().add(offset) as *mut u64).write_volatile(value) }
    }

    fn capacity(&self) -> usize {
        self.read_header_field(offset_of!(Header, capacity))
    }

    fn head(&self) -> usize {
        self.read_header_field(offset_of!(Header, head))
    }

    fn tail(&self) -> usize {
        self.read_header_field(offset_of!(Header, tail))
    }

    fn set_capacity(&mut self, value: usize) {
        self.write_header_field(offset_of!(Header, capacity), value);
    }

    fn set_head(&mut self, value: usize) {
        self.write_header_field(offset_of!(Header, head), value);
    }

    fn set_tail(&mut self, value: usize) {
        self.write_header_field(offset_of!(Header, tail), value);
    }
}

impl<'a> RingBuffer<'a, Producer> {
    /// Writes `data` into the ringbuffer, returning how many bytes were written
    pub fn write(&mut self, data: &[u8]) -> usize {
        let head = self.head();
        let tail = self.tail();

        let capacity = self.capacity();

        if data.len() > capacity {
            panic!("overflow");
        }

        let head_wrapped = head % capacity;
        let tail_wrapped = tail % capacity;

        let (low, high) = self.buffer.split_at_mut(tail_wrapped);

        let (a, b) = match head_wrapped.cmp(&tail_wrapped) {
            Ordering::Greater | Ordering::Equal => {
                // [head_wrapped..capacity] and
                // [0..tail_wrapped]

                (&mut high[head_wrapped - tail_wrapped..], low)
            }

            Ordering::Less => {
                // [head_wrapped..tail_wrapped]
                (&mut low[head_wrapped..], &mut high[0..0])
            }
        };

        // amount of bytes to write
        let total = min(a.len() + b.len(), data.len());

        let first_write = min(a.len(), data.len());

        a[..first_write].copy_from_slice(&data[..first_write]);

        if total > first_write {
            b[..total - first_write].copy_from_slice(&data[first_write..total]);
        }

        self.set_head(head + total);

        total
    }
}

impl<'a> RingBuffer<'a, Consumer> {
    /// Read from the ringbuffer
    pub fn read<F: FnOnce(MaybeSplitBuffer) -> usize>(&mut self, f: F) {
        let head = self.head();
        let tail = self.tail();

        if head == tail {
            return;
        }

        let capacity = self.capacity();

        let head_wrapped = head % capacity;
        let tail_wrapped = tail % capacity;

        let buffer = match head_wrapped.cmp(&tail_wrapped) {
            Ordering::Equal => unreachable!(),
            Ordering::Greater => MaybeSplitBuffer::Single(&self.buffer[tail_wrapped..head_wrapped]),
            Ordering::Less => MaybeSplitBuffer::Split(
                &self.buffer[tail_wrapped..capacity],
                &self.buffer[..head_wrapped],
            ),
        };

        let consumed = f(buffer);

        self.set_tail(consumed + tail);
    }
}

#[derive(Debug)]
#[repr(C)]
struct Header {
    capacity: usize,
    head: usize,
    tail: usize,
}

pub enum MaybeSplitBuffer<'a> {
    Single(&'a [u8]),
    Split(&'a [u8], &'a [u8]),
}

impl<'a> MaybeSplitBuffer<'a> {
    pub fn len(&self) -> usize {
        match self {
            Self::Single(buf) => buf.len(),
            Self::Split(a, b) => a.len() + b.len(),
        }
    }
}
