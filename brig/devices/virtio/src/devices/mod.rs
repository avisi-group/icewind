use {
    crate::queue::VirtQueue,
    alloc::{sync::Arc, vec::Vec},
    bitfields::bitfield,
    common::device::IrqController,
    core::sync::atomic::AtomicU32,
    virtio_drivers::transport::pci::VIRTIO_VENDOR_ID,
};

pub mod block;

const VIRTIO_MAGIC: u32 = u32::from_le_bytes([b'v', b'i', b'r', b't']);
const VIRTIO_VERSION: u32 = 0x2;
const VIRTIO_DEV_BLK: u16 = 0x2;

#[bitfield(u32)]
#[derive(Clone, Copy)]
struct Status {
    /// Indicates that the guest OS has found the device and recognized it as a
    /// valid virtio device.
    acknowledge: bool,
    /// Indicates that the guest OS knows how to drive the device. Note: There
    /// could be a significant (or infinite) delay before setting this bit. For
    /// example, under Linux, drivers can be loadable modules.
    driver: bool,
    /// Indicates that the driver has acknowledged all the features it
    /// understands, and feature negotiation is complete.
    driver_ok: bool,
    /// Indicates that the driver is set up and ready to drive the device.
    features_ok: bool,

    #[bits(2)]
    _reserved: u8,

    /// Indicates that the device has experienced an error from which it can’t
    /// recover.
    device_needs_reset: bool,

    /// Indicates that something went wrong in the guest, and it has given up on
    /// the device. This could be an internal error, or the driver didn’t like
    /// the device for some reason, or even a fatal error during device
    /// operation.
    failed: bool,

    #[bits(24)]
    _reserved: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadRegister {
    Magic,
    Version,
    DeviceId,
    VendorId,
    DeviceFeatures,
    QueueReady,
    QueueNumMax,
    InterruptStatus,
    Status,
    ConfigGeneration,
}

impl ReadRegister {
    fn from_offset(offset: u64) -> Self {
        match offset {
            0x00 => Self::Magic,
            0x04 => Self::Version,
            0x08 => Self::DeviceId,
            0x0c => Self::VendorId,
            0x10 => Self::DeviceFeatures,
            0x34 => Self::QueueNumMax,
            0x44 => Self::QueueReady,
            0x60 => Self::InterruptStatus,
            0x70 => Self::Status,
            0xfc => Self::ConfigGeneration,
            _ => panic!("unknown read reg offset {offset:x}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteRegister {
    DeviceFeaturesSelect,
    DriverFeatures,
    DriverFeaturesSelect,
    QueueSelect,
    QueueNum,
    QueueReady,
    QueueNotify,
    InterruptAcknowledge,
    Status,
    QueueDescriptorLow,
    QueueDescriptorHigh,
    QueueAvailableLow,
    QueueAvailableHigh,
    QueueUsedLow,
    QueueUsedHigh,
}

impl WriteRegister {
    fn from_offset(offset: u64) -> Self {
        match offset {
            0x14 => Self::DeviceFeaturesSelect,
            0x20 => Self::DriverFeatures,
            0x24 => Self::DriverFeaturesSelect,
            0x30 => Self::QueueSelect,
            0x38 => Self::QueueNum,
            0x44 => Self::QueueReady,
            0x50 => Self::QueueNotify,
            0x64 => Self::InterruptAcknowledge,
            0x70 => Self::Status,
            0x80 => Self::QueueDescriptorLow,
            0x84 => Self::QueueDescriptorHigh,
            0x90 => Self::QueueAvailableLow,
            0x94 => Self::QueueAvailableHigh,
            0xa0 => Self::QueueUsedLow,
            0xa4 => Self::QueueUsedHigh,
            _ => panic!("unknown write reg offset {offset:x}"),
        }
    }
}

pub struct Virtio {
    device_id: u16,
    device_feature_select: bool,
    device_features: [u32; 2],
    driver_features_select: u32,
    driver_features: u32,
    isr: AtomicU32,
    status: Status,
    queues: Vec<VirtQueue>,
    queue_select: usize,
    irq: Irq,
}

impl Virtio {
    fn new(
        num_queues: usize,
        device_id: u16,
        irq_line: usize,
        irq_controller: Arc<dyn IrqController>,
        read_callback: fn(&mut [u8], usize),
        write_callback: fn(&[u8], usize),
    ) -> Virtio {
        assert!(num_queues > 0);
        Self {
            device_id,
            device_feature_select: false,
            device_features: [0, 0],
            isr: AtomicU32::new(0),
            status: Status::new(),
            queues: (0..num_queues)
                .into_iter()
                .map(|_| VirtQueue::new(read_callback, write_callback))
                .collect(),
            queue_select: 0,
            driver_features_select: 0,
            driver_features: 0,
            irq: Irq {
                line: irq_line,
                controller: irq_controller,
            },
        }
    }

    fn selected_queue(&self) -> &VirtQueue {
        &self.queues[self.queue_select]
    }
    fn selected_queue_mut(&mut self) -> &mut VirtQueue {
        &mut self.queues[self.queue_select]
    }

    fn selected_device_feature(&self) -> u32 {
        self.device_features[usize::from(self.device_feature_select)]
    }

    fn set_host_feature(&mut self, idx: usize) {
        if idx > 31 {
            self.device_features[1] |= 1 << (idx - 32);
        } else {
            self.device_features[0] |= 1 << idx;
        }
    }

    fn clear_host_feature(&mut self, idx: usize) {
        if idx > 31 {
            self.device_features[1] &= !(1 << (idx - 32));
        } else {
            self.device_features[0] &= !(1 << idx);
        }
    }

    fn reset(&self) {
        // ???
    }

    fn read_register(&self, register: ReadRegister) -> u32 {
        use ReadRegister::*;

        let resp = match register {
            Magic => VIRTIO_MAGIC,
            Version => VIRTIO_VERSION,
            DeviceId => u32::from(self.device_id),
            VendorId => u32::from(VIRTIO_VENDOR_ID),
            DeviceFeatures => self.selected_device_feature(),
            QueueReady => u32::from(self.selected_queue().ready()),
            QueueNumMax => u32::try_from(self.selected_queue().num_max()).unwrap(),
            InterruptStatus => self.isr.load(core::sync::atomic::Ordering::Relaxed),
            Status => self.status.0,
            ConfigGeneration => 0,
        };

        if register == Status {
            log::debug!("read status {:#?}", self.status);
        } else {
            log::debug!("read reg {register:?}: {resp:?}");
        }

        resp
    }

    fn write_register(&mut self, register: WriteRegister, value: u32) {
        use WriteRegister as R;

        if register == R::Status {
            log::debug!("writing status {:#?}", Status::from(value));
        } else {
            log::debug!("write reg {register:?}: {value:?}");
        }

        match register {
            R::DeviceFeaturesSelect => {
                self.device_feature_select = match value {
                    0 => false,
                    1 => true,
                    _ => panic!(),
                };
            }
            R::Status => {
                self.status = Status::from(value);

                if value == 0 {
                    self.driver_features_select = 0;
                    self.driver_features = 0;

                    self.reset();
                }
            }
            R::DriverFeatures => {
                self.driver_features = value;
            }
            R::DriverFeaturesSelect => {
                self.driver_features_select = value;
            }
            R::QueueSelect => {
                let idx = usize::try_from(value).unwrap();
                assert!(idx < self.queues.len());
                self.queue_select = idx;
            }
            R::QueueNum => {
                let num = usize::try_from(value).unwrap();
                self.selected_queue_mut().set_num(num);
            }
            R::QueueReady => {
                let ready = match value {
                    0 => false,
                    1 => true,
                    _ => panic!(),
                };
                self.selected_queue_mut().set_ready(ready);
            }
            R::QueueNotify => {
                let queue_idx = usize::try_from(value).unwrap();
                self.queues[queue_idx].process(&self.irq, &self.isr)
            }
            R::InterruptAcknowledge => {
                let isr = self
                    .isr
                    .fetch_and(!value, core::sync::atomic::Ordering::Relaxed);

                if isr != 0 {
                    log::debug!("ack int raise");
                    self.irq.controller.raise(self.irq.line);
                } else {
                    log::debug!("ack int rescind");
                    self.irq.controller.rescind(self.irq.line);
                }
            }
            R::QueueDescriptorLow => self.selected_queue_mut().set_descriptor_low(value),
            R::QueueDescriptorHigh => self.selected_queue_mut().set_descriptor_high(value),
            R::QueueAvailableLow => self.selected_queue_mut().set_available_low(value),
            R::QueueAvailableHigh => self.selected_queue_mut().set_available_high(value),
            R::QueueUsedLow => self.selected_queue_mut().set_used_low(value),
            R::QueueUsedHigh => self.selected_queue_mut().set_used_high(value),
        }
    }
}

pub struct Irq {
    pub line: usize,
    pub controller: Arc<dyn IrqController>,
}
