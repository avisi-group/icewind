use {
    core::ptr::addr_of,
    spin::Once,
    x86_64::{
        VirtAddr,
        instructions::{
            segmentation::{CS, Segment},
            tables::load_tss,
        },
        registers::segmentation::{DS, ES, FS, GS, SS},
        structures::{
            gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
            tss::TaskStateSegment,
        },
    },
};

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static TSS: Once<TaskStateSegment> = Once::INIT;
static GDT: Once<(GlobalDescriptorTable, Selectors)> = Once::INIT;

struct Selectors {
    kernel_code_selector: SegmentSelector,
    kernel_data_selector: SegmentSelector,
    #[allow(unused)]
    user_data_selector: SegmentSelector,
    #[allow(unused)]
    user_code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

pub fn init() {
    TSS.call_once(|| {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(addr_of!(STACK));
            stack_start + u64::try_from(STACK_SIZE).unwrap() // stack end
        };
        tss
    });

    GDT.call_once(|| {
        let mut gdt = GlobalDescriptorTable::new();
        let kernel_code_selector = gdt.append(Descriptor::kernel_code_segment());
        let kernel_data_selector = gdt.append(Descriptor::kernel_data_segment());
        let user_data_selector = gdt.append(Descriptor::user_data_segment());
        let user_code_selector = gdt.append(Descriptor::user_code_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(TSS.get().unwrap()));
        (
            gdt,
            Selectors {
                kernel_code_selector,
                kernel_data_selector,
                user_data_selector,
                user_code_selector,
                tss_selector,
            },
        )
    });

    GDT.get().unwrap().0.load();
    unsafe {
        CS::set_reg(GDT.get().unwrap().1.kernel_code_selector);
        DS::set_reg(GDT.get().unwrap().1.kernel_data_selector);
        ES::set_reg(GDT.get().unwrap().1.kernel_data_selector);
        FS::set_reg(GDT.get().unwrap().1.kernel_data_selector);
        GS::set_reg(GDT.get().unwrap().1.kernel_data_selector);
        SS::set_reg(GDT.get().unwrap().1.kernel_data_selector);
        load_tss(GDT.get().unwrap().1.tss_selector);
    }
}
