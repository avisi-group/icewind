use {
    crate::arch::x86::{
        gdt::{GDT, TSS},
        memory::{BoxToVirtAddrExt, VirtAddrExt},
        vmx::{
            ept::Ept,
            iobitmap::IoBitmap,
            vmcs::{Vmcs, VmcsError},
        },
    },
    alloc::{alloc::alloc_zeroed, boxed::Box},
    bitset_core::BitSet,
    core::{alloc::Layout, arch::naked_asm},
    x86::{
        bits64::{
            segmentation::{rdfsbase, rdgsbase, wrgsbase},
            vmx::vmxon,
        },
        controlregs::{cr0, cr3, cr4},
        msr::{
            IA32_EFER, IA32_VMX_BASIC, IA32_VMX_CR0_FIXED0, IA32_VMX_CR0_FIXED1,
            IA32_VMX_CR4_FIXED0, IA32_VMX_CR4_FIXED1, IA32_VMX_ENTRY_CTLS, IA32_VMX_EXIT_CTLS,
            IA32_VMX_PINBASED_CTLS, IA32_VMX_PROCBASED_CTLS, IA32_VMX_PROCBASED_CTLS2,
            IA32_VMX_TRUE_ENTRY_CTLS, IA32_VMX_TRUE_EXIT_CTLS, IA32_VMX_TRUE_PINBASED_CTLS,
            IA32_VMX_TRUE_PROCBASED_CTLS, rdmsr,
        },
        vmx::vmcs::control::{
            EntryControls, ExitControls, PinbasedControls, PrimaryControls, SecondaryControls,
        },
    },
    x86_64::{
        PhysAddr, VirtAddr,
        instructions::tables::{sgdt, sidt},
        structures::tss::TaskStateSegment,
    },
};
mod ept;
mod iobitmap;
mod vmcs;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct VmMachineContext {
    rax: u64,
    rcx: u64,
    rdx: u64,
    rbx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

pub fn init() {
    log::error!("starting vmx init");

    let vmxon_region = Vmxon::new();

    unsafe { vmxon(vmxon_region.as_phys().as_u64()).unwrap() };
    log::error!("vmxon");

    // Setup VM context
    let vm_context = Box::new(VmMachineContext::default());
    unsafe {
        wrgsbase(vm_context.as_virt().as_u64());
    }

    // Setup VMCS
    let mut vmcs = Vmcs::new();
    vmcs.activate().unwrap();
    log::error!("activated vmcs");

    // IO Bitmap
    let mut io_bitmap = IoBitmap::new();
    io_bitmap.set_io_exiting(0x0d, false);
    io_bitmap.set_io_exiting(0xda, false);
    io_bitmap.set_io_exiting(0xd6, false);
    io_bitmap.set_io_exiting(0xd4, false);
    io_bitmap.set_io_exiting(0x20, false);
    io_bitmap.set_io_exiting(0x21, false);
    io_bitmap.set_io_exiting(0xa0, false);
    io_bitmap.set_io_exiting(0xa1, false);
    io_bitmap.set_io_exiting(0x92, false);
    io_bitmap.set_io_exiting(0x510, false);

    // EPT
    let mut ept = Ept::new();
    //ept.map_page(0, 0, flags);

    init_vmcs(&mut vmcs, &ept, &io_bitmap).unwrap();
    log::error!("init vmcs");

    // Enter VM loop
    let launched = vmcs.is_launched();
    if !launched {
        vmcs.set_launched();
    }
    log::error!("launched");

    loop {
        log::error!("start of loop");

        x86_64::instructions::interrupts::disable();
        let rc = vmx_run(!launched);
        x86_64::instructions::interrupts::enable();

        if rc != 0 {
            let vm_error = vmcs.read_vm_instruction_error().unwrap();
            panic!("vmx launch or resume failed: {vm_error}");
        }

        log::error!("{}", vmcs);

        panic!(
            "vm exit reason: {}, qualification: {}",
            vmcs.read_vm_exit_reason().unwrap() & 0xffff,
            vmcs.read_exit_qualification().unwrap()
        );
    }
}

pub extern "C" fn inside_the_vm() {
    panic!("SAUSAGE");
}

pub fn compute_vmx_control(msr: u32, required: u32) -> u32 {
    let control = unsafe { rdmsr(msr) };
    let allowed_zero_setting = (control & 0xffff_ffff) as u32;
    let allowed_one_setting = ((control >> 32) & 0xffff_ffff) as u32;

    log::error!(
        "msr={msr:x}, control={control:x}, a0={allowed_zero_setting:x} a1={allowed_one_setting:x} r={required:x}"
    );

    let value = (required & allowed_one_setting) | allowed_zero_setting;

    if (required & !value) != 0 {
        panic!("unsupported requested control bit");
    }

    log::error!("value={value:x}");

    value
}

fn init_vmcs(vmcs: &mut Vmcs, ept: &Ept, io_bitmap: &IoBitmap) -> Result<(), VmcsError> {
    let vmx_basic = unsafe { rdmsr(IA32_VMX_BASIC) };
    let use_true = vmx_basic.bit_test(55);

    log::error!("use true: {use_true}");

    vmcs.write_pin_based_vm_exec_control(
        compute_vmx_control(
            if use_true {
                IA32_VMX_TRUE_PINBASED_CTLS
            } else {
                IA32_VMX_PINBASED_CTLS
            },
            (PinbasedControls::VMX_PREEMPTION_TIMER).bits(),
        )
        .into(),
    )?;

    vmcs.write_vm_entry_controls(
        compute_vmx_control(
            if use_true {
                IA32_VMX_TRUE_ENTRY_CTLS
            } else {
                IA32_VMX_ENTRY_CTLS
            },
            (EntryControls::IA32E_MODE_GUEST).bits(),
        )
        .into(),
    )?;

    vmcs.write_vm_exit_controls(
        compute_vmx_control(
            if use_true {
                IA32_VMX_TRUE_EXIT_CTLS
            } else {
                IA32_VMX_EXIT_CTLS
            },
            (ExitControls::HOST_ADDRESS_SPACE_SIZE).bits(),
        )
        .into(),
    )?;

    vmcs.write_cpu_based_vm_exec_control(
        compute_vmx_control(
            if use_true {
                IA32_VMX_TRUE_PROCBASED_CTLS
            } else {
                IA32_VMX_PROCBASED_CTLS
            },
            (PrimaryControls::SECONDARY_CONTROLS).bits(),
        )
        .into(),
    )?;

    vmcs.write_secondary_vm_exec_control(
        compute_vmx_control(
            IA32_VMX_PROCBASED_CTLS2,
            (SecondaryControls::ENABLE_EPT).bits(),
        )
        .into(),
    )?;

    vmcs.write_page_fault_error_code_mask(0)?;
    vmcs.write_page_fault_error_code_mask(0)?;
    vmcs.write_cr3_target_count(0)?;
    vmcs.write_vmcs_link_pointer(!0)?;
    vmcs.write_virtual_processor_id(0)?;
    vmcs.write_exception_bitmap(0)?;
    vmcs.write_apic_access_addr(0xfee00000)?; // TODO: Read host
    let virtual_apic = allocate_page().to_phys().as_u64();
    vmcs.write_virtual_apic_page_addr(virtual_apic)?;
    vmcs.write_vmx_preemption_timer_value(0xffff_ffff)?;

    vmcs.write_io_bitmap_a(io_bitmap.get_a_phys())?;
    vmcs.write_io_bitmap_b(io_bitmap.get_b_phys())?;

    vmcs.write_ept_pointer(ept.phys_addr().as_u64() | 0x1e)?;

    // Host State
    vmcs.write_host_cr0(u64::try_from(unsafe { cr0().bits() }).unwrap())?;
    vmcs.write_host_cr3(u64::try_from(unsafe { cr3() }).unwrap())?;
    vmcs.write_host_cr4(u64::try_from(unsafe { cr4().bits() }).unwrap())?;
    vmcs.write_host_fs_base(unsafe { rdfsbase() })?;
    vmcs.write_host_gs_base(unsafe { rdgsbase() })?;
    vmcs.write_host_rip(vmx_exit as *const fn() as u64)?;

    vmcs.write_host_cs_selector(GDT.get().unwrap().1.kernel_code_selector.0.into())?;
    vmcs.write_host_ds_selector(GDT.get().unwrap().1.kernel_data_selector.0.into())?;
    vmcs.write_host_es_selector(GDT.get().unwrap().1.kernel_data_selector.0.into())?;
    vmcs.write_host_fs_selector(GDT.get().unwrap().1.kernel_data_selector.0.into())?;
    vmcs.write_host_gs_selector(GDT.get().unwrap().1.kernel_data_selector.0.into())?;
    vmcs.write_host_ss_selector(GDT.get().unwrap().1.kernel_data_selector.0.into())?;
    vmcs.write_host_tr_selector(GDT.get().unwrap().1.tss_selector.0.into())?;

    vmcs.write_host_ia32_efer(unsafe { rdmsr(IA32_EFER) })?;
    vmcs.write_host_ia32_sysenter_cs(0)?;

    let idt_ptr = sidt();
    let gdt_ptr = sgdt();

    vmcs.write_host_idtr_base(idt_ptr.base.as_u64())?;
    vmcs.write_host_gdtr_base(gdt_ptr.base.as_u64())?;
    vmcs.write_host_tr_base(TSS.get().unwrap() as *const TaskStateSegment as u64)?;

    // Guest State
    let fixed_cr0_set = unsafe { rdmsr(IA32_VMX_CR0_FIXED0) };
    let fixed_cr0_clr = unsafe { !rdmsr(IA32_VMX_CR0_FIXED1) };
    let cr0val = (vmcs.read_host_cr0()? & !fixed_cr0_clr) | fixed_cr0_set;

    vmcs.write_guest_cr0(cr0val)?;

    vmcs.write_guest_cr3(vmcs.read_host_cr3().unwrap())?;

    let fixed_cr4_set = unsafe { rdmsr(IA32_VMX_CR4_FIXED0) };
    let fixed_cr4_clr = unsafe { !rdmsr(IA32_VMX_CR4_FIXED1) };
    let cr4val = (vmcs.read_host_cr4()? & !fixed_cr4_clr) | fixed_cr4_set;

    vmcs.write_guest_cr4(cr4val)?;

    vmcs.write_guest_rip(inside_the_vm as *const fn() as u64)?;

    vmcs.write_guest_ia32_efer(vmcs.read_host_ia32_efer().unwrap())?;

    //  Bits 3:0 (Type).
    // • CS. The values allowed depend on the setting of the “unrestricted guest”
    // VM-execution control:
    // — If the control is 0, the Type must be 9, 11, 13, or 15 (accessed code
    // segment).
    // — If the control is 1, the Type must be either 3 (read/write
    // accessed expand-up data segment) or one of 9, 11, 13, and 15 (accessed
    // code segment).
    // • SS. If SS is usable, the Type must be 3 or 7
    // (read/write, accessed data segment). • DS, ES, FS, GS. The following
    // checks apply if the register is usable: — Bit 0 of the Type must be 1
    // (accessed). — If bit 3 of the Type is 1 (code segment), then bit 1 of the
    // Type must be 1 (readable).

    vmcs.write_guest_cs_selector(vmcs.read_host_cs_selector()?)?;
    vmcs.write_guest_cs_base(0)?;
    vmcs.write_guest_cs_limit(0xf_ffff)?;
    vmcs.write_guest_cs_ar_bytes(0b10_0000_1001_1011)?;

    vmcs.write_guest_ds_selector(vmcs.read_host_ds_selector()?)?;
    vmcs.write_guest_ds_base(0)?;
    vmcs.write_guest_ds_limit(0xf_ffff)?;
    vmcs.write_guest_ds_ar_bytes(0b1001_0011)?;

    vmcs.write_guest_es_selector(vmcs.read_host_es_selector()?)?;
    vmcs.write_guest_es_base(0)?;
    vmcs.write_guest_es_limit(0xf_ffff)?;
    vmcs.write_guest_es_ar_bytes(0b1001_0011)?;

    vmcs.write_guest_fs_selector(vmcs.read_host_fs_selector()?)?;
    vmcs.write_guest_fs_base(0)?;
    vmcs.write_guest_fs_limit(0xf_ffff)?;
    vmcs.write_guest_fs_ar_bytes(0b1001_0011)?;

    vmcs.write_guest_gs_selector(vmcs.read_host_gs_selector()?)?;
    vmcs.write_guest_gs_base(0)?;
    vmcs.write_guest_gs_limit(0xf_ffff)?;
    vmcs.write_guest_gs_ar_bytes(0b1001_0011)?;

    // bit 6:5 (DPL)
    //     SS.
    // — If the “unrestricted guest” VM-execution control is 0, the DPL must equal
    // the RPL from the selector field.
    // — The DPL must be 0 either if the Type in the access-rights field for CS is 3
    // (read/write accessed expand-up data segment) or if bit 0 in the CR0 field
    // (corresponding to CR0.PE) is 0.1
    vmcs.write_guest_ss_selector(vmcs.read_host_ss_selector()?)?;
    vmcs.write_guest_ss_base(0)?;
    vmcs.write_guest_ss_limit(0xf_ffff)?;
    vmcs.write_guest_ss_ar_bytes(0b1001_0011)?;

    vmcs.write_guest_tr_selector(vmcs.read_host_tr_selector()?)?;
    vmcs.write_guest_tr_base(vmcs.read_host_tr_base()?)?;
    vmcs.write_guest_tr_limit((size_of::<TaskStateSegment>() - 1) as u64)?;
    vmcs.write_guest_tr_ar_bytes(0x8b)?;

    vmcs.write_guest_ldtr_selector(0)?;
    vmcs.write_guest_ldtr_base(0)?;
    vmcs.write_guest_ldtr_limit(0)?;
    vmcs.write_guest_ldtr_ar_bytes(0x82)?;

    // The following checks are performed on the fields for GDTR and IDTR:
    // * On processors that support Intel 64 architecture, the base-address fields
    //   must contain canonical addresses.
    // * Bits 31:16 of each limit field must be 0.
    vmcs.write_guest_idtr_base(vmcs.read_host_idtr_base()?)?;
    vmcs.write_guest_idtr_limit(idt_ptr.limit as u64)?;
    vmcs.write_guest_gdtr_base(vmcs.read_host_gdtr_base()?)?;
    vmcs.write_guest_gdtr_limit(gdt_ptr.limit as u64)?;

    vmcs.write_guest_rflags(2)?;

    //    panic!("{vmcs}");

    Ok(())
}

/// A representation of the VMXON region in memory.
///
/// The VMXON region is essential for enabling VMX operations on the CPU.
/// This structure offers methods for setting up the VMXON region, enabling VMX
/// operations, and performing related tasks.
///
/// Reference: Intel® 64 and IA-32 Architectures Software Developer's Manual:
/// 25.11.5 VMXON Region
#[repr(C, align(4096))]
pub struct Vmxon {
    /// Revision ID required for VMXON.
    pub revision_id: u32,

    /// Data array constituting the rest of the VMXON region.
    pub data: [u8; 4096 - 4],
}

impl Vmxon {
    /// Initializes the VMXON region.
    pub fn new() -> Box<Self> {
        // clear bit 31
        let revision_id = (unsafe { rdmsr(IA32_VMX_BASIC) } as u32) & !(1 << 31);

        Box::new(Self {
            revision_id,
            data: [0u8; _],
        })
    }

    pub fn as_phys(self: &Box<Self>) -> PhysAddr {
        self.as_virt().to_phys()
    }
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn vmx_enter(launch: bool) {
    naked_asm!(
        "
            // Set HOST_RSP
            mov $0x6c14, %eax
            vmwrite %rsp, %rax

            // vmlaunch, or vmresume?
            test %edi, %edi

            // Now, load state (without affecting flags)
            mov %gs:0,   %rax
            mov %gs:8,   %rcx
            mov %gs:16,  %rdx
            mov %gs:24,  %rbx
            mov %gs:32,  %rsi
            mov %gs:40,  %rdi
            mov %gs:48,  %rbp
            mov %gs:56,  %r8
            mov %gs:64,  %r9
            mov %gs:72,  %r10
            mov %gs:80,  %r11
            mov %gs:88,  %r12
            mov %gs:96,  %r13
            mov %gs:104, %r14
            mov %gs:112, %r15

            // Jump if we should do a vmlaunch
            jnz 1f

            // Enter the VM
            vmresume
            jmp 2f

        1:
            // Enter the VM
            vmlaunch

        2:
            // If we get here, then the vmresume/vmlaunch failed, and we don't need to
            // save the guest state.
            xor %eax, %eax
            setbe %al
            ret
        ",
        options(att_syntax)
    )
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn vmx_exit() {
    naked_asm!(
        "
            // If we get here, then the VM exited guest mode, so we need to
            // save the guest state.  %gs will be loaded with the guest state
            // pointer, so save register values directly into that area.
            // RSP, RFLAGS, and RIP aren't stored here, as they are part of the VMCS.
            mov %rax, %gs:0
            mov %rcx, %gs:8
            mov %rdx, %gs:16
            mov %rbx, %gs:24
            mov %rsi, %gs:32
            mov %rdi, %gs:40
            mov %rbp, %gs:48
            mov %r8,  %gs:56
            mov %r9,  %gs:64
            mov %r10, %gs:72
            mov %r11, %gs:80
            mov %r12, %gs:88
            mov %r13, %gs:96
            mov %r14, %gs:104
            mov %r15, %gs:112

            // Return zero.  Because of how the stack is set up, we should
            // be returning to vmx_run.
            xor %eax, %eax
            ret
        ",
        options(att_syntax)
    )
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn vmx_run(launch: bool) -> u32 {
    naked_asm!(
        "
            // Need to save/restore all callee saved regs, as everything gets
            // clobbered.

            push %rbp
            push %rbx
            push %r12
            push %r13
            push %r14
            push %r15

            call vmx_enter

            pop %r15
            pop %r14
            pop %r13
            pop %r12
            pop %rbx
            pop %rbp

            // RAX was setup with the appropriate return value via vmx_enter (if it failed),
            // or zero if we are returning via vmx_exit.
            ret
        ",
        options(att_syntax)
    )
}

fn allocate_page() -> VirtAddr {
    VirtAddr::from_ptr(unsafe { alloc_zeroed(Layout::from_size_align(4096, 4096).unwrap()) })
}
