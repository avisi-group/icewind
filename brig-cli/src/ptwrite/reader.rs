use {
    crate::ptwrite::{
        decoder::Decoder,
        ring_buffer::RingBufferAux,
        thread_handle::{Context, ThreadHandle},
    },
    memmap2::{Mmap, MmapMut, RemapOptions},
    perf_event_open_sys::{
        bindings::{perf_event_attr, perf_event_mmap_page},
        perf_event_open,
    },
    std::{
        fs::File,
        io::{BufWriter, Read, Write},
        path::{Path, PathBuf},
        process,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicI32, Ordering},
        },
    },
};

const PAGE_SIZE: usize = 4096;

/// Path to the value of the current Intel PT type
const INTEL_PT_TYPE_PATH: &str = "/sys/bus/event_source/devices/intel_pt/type";

pub const BUFFER_SIZE: usize = 2 * 1024 * 1024 * 1024;
const NR_DATA_PAGES: usize = 256;

//pub static READY: AtomicBool = AtomicBool::new(false);

pub struct Reader {
    handle: ThreadHandle,
}

impl Reader {
    pub fn init<P: AsRef<Path> + Send + 'static>(
        path: P,
        target_pid: i32,
    ) -> (Self, Arc<AtomicI32>) {
        let perf_file_descriptor = Arc::new(AtomicI32::new(-1));
        let fd = perf_file_descriptor.clone();
        (
            Self {
                handle: ThreadHandle::spawn(move |ctx| read_pt_data(ctx, target_pid, fd, path)),
            },
            perf_file_descriptor,
        )
    }

    pub fn exit(self) {
        self.handle.exit();
    }
}

fn read_pt_data<P: AsRef<Path>>(
    ctx: Context,
    target_pid: i32,
    perf_file_descriptor: Arc<AtomicI32>,
    path: P,
) {
    let mut pea = perf_event_attr::default();

    // perf event type
    pea.type_ = get_intel_pt_perf_type();

    // Event should start disabled
    // 2026-07-22 fmckeogh: why?
    pea.set_disabled(1);

    // we now *are* in a vm and recording kernel so turn off exclusions?
    pea.set_exclude_kernel(0);
    pea.set_exclude_hv(1);
    pea.set_exclude_guest(0);
    pea.set_exclude_host(1);
    pea.set_exclude_user(1);

    pea.set_inherit(0);

    pea.set_sample_id_all(0);

    // still needed?
    pea.set_precise_ip(0);

    // 0 pt
    // 1 cyc
    // 2
    // 3

    // 4 pwr_evt
    // 5 fup_on_ptw
    // 7
    // 8

    // 9 mtc
    // 10 tsc
    // 11 noretcomp
    // 12 ptw

    // 13 branch
    // 14-17 mtc_period

    // 19-22 cyc_thresh

    // 24-27 psb_period

    // 31 event

    // 55 notnt
    pea.config = (1 << 0) | (1 << 12);

    pea.size = std::mem::size_of::<perf_event_attr>() as u32;

    {
        let result = unsafe {
            perf_event_open(
                (&mut pea) as *mut _,
                target_pid,
                -1,
                -1,
                perf_event_open_sys::bindings::PERF_FLAG_FD_CLOEXEC.into(),
            )
        };
        if result < 0 {
            println!("last OS error: {:?}", std::io::Error::last_os_error());
            panic!("perf_event_open failed {result}");
        }
        perf_file_descriptor.store(result, Ordering::Relaxed);
    }

    let mmap = memmap2::MmapOptions::new()
        .len((NR_DATA_PAGES + 1) * PAGE_SIZE)
        .map_raw(perf_file_descriptor.load(Ordering::Relaxed))
        .unwrap();

    let header = unsafe { &mut *(mmap.as_mut_ptr() as *mut perf_event_mmap_page) };

    header.aux_offset = header.data_offset + header.data_size;
    header.aux_size = BUFFER_SIZE as u64;

    let aux_area = memmap2::MmapOptions::new()
        .len(header.aux_size as usize)
        .offset(header.aux_offset)
        .map_raw(perf_file_descriptor.load(Ordering::Relaxed))
        .unwrap();

    let mut ring_buffer_aux = RingBufferAux::new(mmap, aux_area);

    let mut terminating = false;

    let mut f = BufWriter::new(File::create(&path).unwrap());
    // let mut sink = MmapSink::new(&path);

    ctx.ready();

    let mut total = 0;

    loop {
        let did_process_data = ring_buffer_aux.next_data(|main, secondary| {
            let mut decoder = Decoder::new(main, secondary);

            while let Some(ip) = decoder.next() {
                f.write_all(&ip.to_ne_bytes()).unwrap()
            }

            let consumed = decoder.offset();
            total += consumed;

            consumed
        });

        // only perform additional logic if we consumed 0, otherwise immediately call
        // next data
        if !did_process_data {
            // if we consumed nothing and are terminating, exit
            if terminating {
                log::trace!("read terminating");
                dbg!(total);

                return;
            }

            // received nothing and received exit -> start terminating
            if ctx.received_exit() {
                log::trace!("reader received exit");
                terminating = true;
                continue;
            }
        }
    }
}

fn get_intel_pt_perf_type() -> u32 {
    let mut intel_pt_type = File::open(INTEL_PT_TYPE_PATH).unwrap();

    let mut buf = String::new();
    intel_pt_type.read_to_string(&mut buf).unwrap();

    buf.trim().parse().unwrap()
}

struct MmapSink {
    file: File,
    map: MmapMut,
    pos: usize,
    size: usize,
}

impl MmapSink {
    const RESIZE_AMOUNT: usize = 4 * 1024 * 1024;

    pub fn new(path: &Path) -> Self {
        let file = File::options()
            .read(true)
            .write(true)
            .truncate(true)
            .create(true)
            .open(path)
            .unwrap();
        let map = unsafe { MmapMut::map_mut(&file) }.unwrap();

        let mut celf = Self {
            file,
            map,
            pos: 0,
            size: 0,
        };

        celf.resize();

        celf
    }

    fn resize(&mut self) {
        let new_size = self.size + Self::RESIZE_AMOUNT;
        self.file.set_len(new_size as u64).unwrap();
        unsafe { self.map.remap(new_size, RemapOptions::new().may_move(true)) }.unwrap();
        self.size = new_size;
    }

    pub fn push(&mut self, value: u64) {
        if self.pos > self.size - Self::RESIZE_AMOUNT {
            self.resize();
        }

        self.map[self.pos..self.pos + size_of::<u64>()].copy_from_slice(&value.to_ne_bytes());
        self.pos += size_of::<u64>();
    }
}
