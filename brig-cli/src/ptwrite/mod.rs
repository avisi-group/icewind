use {
    crate::ptwrite::reader::Reader,
    std::{
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicI32, Ordering},
        },
    },
};

pub mod decoder;
pub mod reader;
pub mod ring_buffer;
pub mod thread_handle;

pub struct HardwareTracer {
    pub perf_file_descriptor: Arc<AtomicI32>,

    /// PT reader
    reader: Reader,
}

impl HardwareTracer {
    pub fn init<P: AsRef<Path> + Send + 'static>(path: P, target_pid: i32) -> Self {
        let (reader, perf_file_descriptor) = Reader::init(path, target_pid);

        HardwareTracer {
            perf_file_descriptor,
            reader,
        }
    }

    pub fn start_recording(&self) {
        // READY.store(false, Ordering::Relaxed);
        // while !READY.load(Ordering::Relaxed) {
        //     unsafe { std::arch::asm!("nop") };
        //     // println!("waiting");
        // }

        if unsafe {
            perf_event_open_sys::ioctls::RESET(self.perf_file_descriptor.load(Ordering::Relaxed), 0)
        } < 0
        {
            panic!("failed to start recording");
        }

        if unsafe {
            perf_event_open_sys::ioctls::ENABLE(
                self.perf_file_descriptor.load(Ordering::Relaxed),
                0,
            )
        } < 0
        {
            panic!("failed to start recording");
        }
    }

    pub fn stop_recording(&self) {
        if unsafe {
            perf_event_open_sys::ioctls::DISABLE(
                self.perf_file_descriptor.load(Ordering::Relaxed),
                0,
            )
        } < 0
        {
            panic!("failed to stop recording");
        }
    }

    pub fn exit(self) {
        log::trace!("reader exiting");
        self.reader.exit();
    }
}
