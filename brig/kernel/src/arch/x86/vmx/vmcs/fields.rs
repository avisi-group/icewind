use {
    crate::arch::x86::vmx::vmcs::{Vmcs, VmcsError},
    core::fmt::Display,
    paste::paste,
    x86::vmx::vmcs::{control, guest, host, ro},
};

macro_rules! expose_field {
    ($name:ident, $offset:expr) => {
        paste! {
            pub fn [<read_ $name>](&self) -> Result<u64, VmcsError> {
                self.get_field($offset)
            }
            pub fn [<write_ $name>](&self, value: u64) -> Result<(), VmcsError> {
                self.set_field($offset, value)
            }
        }
    };
}

impl Vmcs {
    //expose_field!(host_rip, host::RIP);
    expose_field!(virtual_processor_id, control::VPID);
    expose_field!(
        posted_intr_nv,
        control::POSTED_INTERRUPT_NOTIFICATION_VECTOR
    );
    expose_field!(guest_es_selector, guest::ES_SELECTOR);
    expose_field!(guest_cs_selector, guest::CS_SELECTOR);
    expose_field!(guest_ss_selector, guest::SS_SELECTOR);
    expose_field!(guest_ds_selector, guest::DS_SELECTOR);
    expose_field!(guest_fs_selector, guest::FS_SELECTOR);
    expose_field!(guest_gs_selector, guest::GS_SELECTOR);
    expose_field!(guest_ldtr_selector, guest::LDTR_SELECTOR);
    expose_field!(guest_tr_selector, guest::TR_SELECTOR);
    expose_field!(guest_intr_status, guest::INTERRUPT_STATUS);
    expose_field!(guest_pml_index, guest::PML_INDEX);
    expose_field!(host_es_selector, host::ES_SELECTOR);
    expose_field!(host_cs_selector, host::CS_SELECTOR);
    expose_field!(host_ss_selector, host::SS_SELECTOR);
    expose_field!(host_ds_selector, host::DS_SELECTOR);
    expose_field!(host_fs_selector, host::FS_SELECTOR);
    expose_field!(host_gs_selector, host::GS_SELECTOR);
    expose_field!(host_tr_selector, host::TR_SELECTOR);
    expose_field!(io_bitmap_a, control::IO_BITMAP_A_ADDR_FULL);
    expose_field!(io_bitmap_a_high, control::IO_BITMAP_A_ADDR_HIGH);
    expose_field!(io_bitmap_b, control::IO_BITMAP_B_ADDR_FULL);
    expose_field!(io_bitmap_b_high, control::IO_BITMAP_B_ADDR_HIGH);
    expose_field!(msr_bitmap, control::MSR_BITMAPS_ADDR_FULL);
    expose_field!(msr_bitmap_high, control::MSR_BITMAPS_ADDR_HIGH);
    expose_field!(vm_exit_msr_store_addr, control::VMEXIT_MSR_STORE_ADDR_FULL);
    expose_field!(
        vm_exit_msr_store_addr_high,
        control::VMEXIT_MSR_STORE_ADDR_HIGH
    );
    expose_field!(vm_exit_msr_load_addr, control::VMEXIT_MSR_LOAD_ADDR_FULL);
    expose_field!(
        vm_exit_msr_load_addr_high,
        control::VMEXIT_MSR_LOAD_ADDR_HIGH
    );
    expose_field!(vm_entry_msr_load_addr, control::VMENTRY_MSR_LOAD_ADDR_FULL);
    expose_field!(
        vm_entry_msr_load_addr_high,
        control::VMENTRY_MSR_LOAD_ADDR_HIGH
    );
    expose_field!(pml_address, control::PML_ADDR_FULL);
    expose_field!(pml_address_high, control::PML_ADDR_HIGH);
    expose_field!(tsc_offset, control::TSC_OFFSET_FULL);
    expose_field!(tsc_offset_high, control::TSC_OFFSET_HIGH);
    expose_field!(virtual_apic_page_addr, control::VIRT_APIC_ADDR_FULL);
    expose_field!(virtual_apic_page_addr_high, control::VIRT_APIC_ADDR_HIGH);
    expose_field!(apic_access_addr, control::APIC_ACCESS_ADDR_FULL);
    expose_field!(apic_access_addr_high, control::APIC_ACCESS_ADDR_HIGH);
    expose_field!(
        posted_intr_desc_addr,
        control::POSTED_INTERRUPT_DESC_ADDR_FULL
    );
    expose_field!(
        posted_intr_desc_addr_high,
        control::POSTED_INTERRUPT_DESC_ADDR_HIGH
    );
    expose_field!(vm_function_control, control::VM_FUNCTION_CONTROLS_FULL);
    expose_field!(vm_function_control_high, control::VM_FUNCTION_CONTROLS_HIGH);
    expose_field!(ept_pointer, control::EPTP_FULL);
    expose_field!(ept_pointer_high, control::EPTP_HIGH);
    expose_field!(eoi_exit_bitmap0, control::EOI_EXIT0_FULL);
    expose_field!(eoi_exit_bitmap0_high, control::EOI_EXIT0_HIGH);
    expose_field!(eoi_exit_bitmap1, control::EOI_EXIT1_FULL);
    expose_field!(eoi_exit_bitmap1_high, control::EOI_EXIT1_HIGH);
    expose_field!(eoi_exit_bitmap2, control::EOI_EXIT2_FULL);
    expose_field!(eoi_exit_bitmap2_high, control::EOI_EXIT2_HIGH);
    expose_field!(eoi_exit_bitmap3, control::EOI_EXIT3_FULL);
    expose_field!(eoi_exit_bitmap3_high, control::EOI_EXIT3_HIGH);
    expose_field!(eptp_list_address, control::EPTP_LIST_ADDR_FULL);
    expose_field!(eptp_list_address_high, control::EPTP_LIST_ADDR_HIGH);
    expose_field!(vmread_bitmap, control::VMREAD_BITMAP_ADDR_FULL);
    expose_field!(vmread_bitmap_high, control::VMREAD_BITMAP_ADDR_HIGH);
    expose_field!(vmwrite_bitmap, control::VMWRITE_BITMAP_ADDR_FULL);
    expose_field!(vmwrite_bitmap_high, control::VMWRITE_BITMAP_ADDR_HIGH);
    expose_field!(xss_exit_bitmap, control::XSS_EXITING_BITMAP_FULL);
    expose_field!(xss_exit_bitmap_high, control::XSS_EXITING_BITMAP_HIGH);
    expose_field!(encls_exiting_bitmap, control::ENCLS_EXITING_BITMAP_FULL);
    expose_field!(
        encls_exiting_bitmap_high,
        control::ENCLS_EXITING_BITMAP_HIGH
    );
    expose_field!(tsc_multiplier, control::TSC_MULTIPLIER_FULL);
    expose_field!(tsc_multiplier_high, control::TSC_MULTIPLIER_HIGH);
    expose_field!(guest_physical_address, ro::GUEST_PHYSICAL_ADDR_FULL);
    expose_field!(guest_physical_address_high, ro::GUEST_PHYSICAL_ADDR_HIGH);
    expose_field!(vmcs_link_pointer, guest::LINK_PTR_FULL);
    expose_field!(vmcs_link_pointer_high, guest::LINK_PTR_HIGH);
    expose_field!(guest_ia32_debugctl, guest::IA32_DEBUGCTL_FULL);
    expose_field!(guest_ia32_debugctl_high, guest::IA32_DEBUGCTL_HIGH);
    expose_field!(guest_ia32_pat, guest::IA32_PAT_FULL);
    expose_field!(guest_ia32_pat_high, guest::IA32_PAT_HIGH);
    expose_field!(guest_ia32_efer, guest::IA32_EFER_FULL);
    expose_field!(guest_ia32_efer_high, guest::IA32_EFER_HIGH);
    expose_field!(
        guest_ia32_perf_global_ctrl,
        guest::IA32_PERF_GLOBAL_CTRL_FULL
    );
    expose_field!(
        guest_ia32_perf_global_ctrl_high,
        guest::IA32_PERF_GLOBAL_CTRL_HIGH
    );
    expose_field!(guest_pdptr0, guest::PDPTE0_FULL);
    expose_field!(guest_pdptr0_high, guest::PDPTE0_HIGH);
    expose_field!(guest_pdptr1, guest::PDPTE1_FULL);
    expose_field!(guest_pdptr1_high, guest::PDPTE1_HIGH);
    expose_field!(guest_pdptr2, guest::PDPTE2_FULL);
    expose_field!(guest_pdptr2_high, guest::PDPTE2_HIGH);
    expose_field!(guest_pdptr3, guest::PDPTE3_FULL);
    expose_field!(guest_pdptr3_high, guest::PDPTE3_HIGH);
    expose_field!(guest_bndcfgs, guest::IA32_BNDCFGS_FULL);
    expose_field!(guest_bndcfgs_high, guest::IA32_BNDCFGS_HIGH);
    expose_field!(guest_ia32_rtit_ctl, guest::IA32_RTIT_CTL_FULL);
    expose_field!(guest_ia32_rtit_ctl_high, guest::IA32_RTIT_CTL_HIGH);
    expose_field!(host_ia32_pat, host::IA32_PAT_FULL);
    expose_field!(host_ia32_pat_high, host::IA32_PAT_HIGH);
    expose_field!(host_ia32_efer, host::IA32_EFER_FULL);
    expose_field!(host_ia32_efer_high, host::IA32_EFER_HIGH);
    expose_field!(host_ia32_perf_global_ctrl, host::IA32_PERF_GLOBAL_CTRL_FULL);
    expose_field!(
        host_ia32_perf_global_ctrl_high,
        host::IA32_PERF_GLOBAL_CTRL_HIGH
    );
    expose_field!(pin_based_vm_exec_control, control::PINBASED_EXEC_CONTROLS);
    expose_field!(
        cpu_based_vm_exec_control,
        control::PRIMARY_PROCBASED_EXEC_CONTROLS
    );
    expose_field!(exception_bitmap, control::EXCEPTION_BITMAP);
    expose_field!(
        page_fault_error_code_mask,
        control::PAGE_FAULT_ERR_CODE_MASK
    );
    expose_field!(
        page_fault_error_code_match,
        control::PAGE_FAULT_ERR_CODE_MATCH
    );
    expose_field!(cr3_target_count, control::CR3_TARGET_COUNT);
    expose_field!(vm_exit_controls, control::VMEXIT_CONTROLS);
    expose_field!(vm_exit_msr_store_count, control::VMEXIT_MSR_STORE_COUNT);
    expose_field!(vm_exit_msr_load_count, control::VMEXIT_MSR_LOAD_COUNT);
    expose_field!(vm_entry_controls, control::VMENTRY_CONTROLS);
    expose_field!(vm_entry_msr_load_count, control::VMENTRY_MSR_LOAD_COUNT);
    expose_field!(
        vm_entry_intr_info_field,
        control::VMENTRY_INTERRUPTION_INFO_FIELD
    );
    expose_field!(
        vm_entry_exception_error_code,
        control::VMENTRY_EXCEPTION_ERR_CODE
    );
    expose_field!(vm_entry_instruction_len, control::VMENTRY_INSTRUCTION_LEN);
    expose_field!(tpr_threshold, control::TPR_THRESHOLD);
    expose_field!(
        secondary_vm_exec_control,
        control::SECONDARY_PROCBASED_EXEC_CONTROLS
    );
    expose_field!(ple_gap, control::PLE_GAP);
    expose_field!(ple_window, control::PLE_WINDOW);

    expose_field!(vm_exit_reason, ro::EXIT_REASON);
    expose_field!(vm_exit_intr_info, ro::VMEXIT_INTERRUPTION_INFO);
    expose_field!(vm_exit_intr_error_code, ro::VMEXIT_INTERRUPTION_ERR_CODE);
    expose_field!(idt_vectoring_info_field, ro::IDT_VECTORING_INFO);
    expose_field!(idt_vectoring_error_code, ro::IDT_VECTORING_ERR_CODE);
    expose_field!(vm_exit_instruction_len, ro::VMEXIT_INSTRUCTION_LEN);
    expose_field!(vm_exit_instruction_info, ro::VMEXIT_INSTRUCTION_INFO);
    expose_field!(guest_es_limit, guest::ES_LIMIT);
    expose_field!(guest_cs_limit, guest::CS_LIMIT);
    expose_field!(guest_ss_limit, guest::SS_LIMIT);
    expose_field!(guest_ds_limit, guest::DS_LIMIT);
    expose_field!(guest_fs_limit, guest::FS_LIMIT);
    expose_field!(guest_gs_limit, guest::GS_LIMIT);
    expose_field!(guest_ldtr_limit, guest::LDTR_LIMIT);
    expose_field!(guest_tr_limit, guest::TR_LIMIT);
    expose_field!(guest_gdtr_limit, guest::GDTR_LIMIT);
    expose_field!(guest_idtr_limit, guest::IDTR_LIMIT);
    expose_field!(guest_es_ar_bytes, guest::ES_ACCESS_RIGHTS);
    expose_field!(guest_cs_ar_bytes, guest::CS_ACCESS_RIGHTS);
    expose_field!(guest_ss_ar_bytes, guest::SS_ACCESS_RIGHTS);
    expose_field!(guest_ds_ar_bytes, guest::DS_ACCESS_RIGHTS);
    expose_field!(guest_fs_ar_bytes, guest::FS_ACCESS_RIGHTS);
    expose_field!(guest_gs_ar_bytes, guest::GS_ACCESS_RIGHTS);
    expose_field!(guest_ldtr_ar_bytes, guest::LDTR_ACCESS_RIGHTS);
    expose_field!(guest_tr_ar_bytes, guest::TR_ACCESS_RIGHTS);
    expose_field!(guest_interruptibility_state, guest::INTERRUPTIBILITY_STATE);
    expose_field!(guest_activity_state, guest::ACTIVITY_STATE);
    expose_field!(guest_sysenter_cs, guest::IA32_SYSENTER_CS);
    expose_field!(
        vmx_preemption_timer_value,
        guest::VMX_PREEMPTION_TIMER_VALUE
    );
    expose_field!(host_ia32_sysenter_cs, host::IA32_SYSENTER_CS);
    expose_field!(cr0_guest_host_mask, control::CR0_GUEST_HOST_MASK);
    expose_field!(cr4_guest_host_mask, control::CR4_GUEST_HOST_MASK);
    expose_field!(cr0_read_shadow, control::CR0_READ_SHADOW);
    expose_field!(cr4_read_shadow, control::CR4_READ_SHADOW);
    expose_field!(cr3_target_value0, control::CR3_TARGET_VALUE0);
    expose_field!(cr3_target_value1, control::CR3_TARGET_VALUE1);
    expose_field!(cr3_target_value2, control::CR3_TARGET_VALUE2);
    expose_field!(cr3_target_value3, control::CR3_TARGET_VALUE3);
    expose_field!(exit_qualification, ro::EXIT_QUALIFICATION);
    expose_field!(guest_linear_address, ro::GUEST_LINEAR_ADDR);
    expose_field!(guest_cr0, guest::CR0);
    expose_field!(guest_cr3, guest::CR3);
    expose_field!(guest_cr4, guest::CR4);
    expose_field!(guest_es_base, guest::ES_BASE);
    expose_field!(guest_cs_base, guest::CS_BASE);
    expose_field!(guest_ss_base, guest::SS_BASE);
    expose_field!(guest_ds_base, guest::DS_BASE);
    expose_field!(guest_fs_base, guest::FS_BASE);
    expose_field!(guest_gs_base, guest::GS_BASE);
    expose_field!(guest_ldtr_base, guest::LDTR_BASE);
    expose_field!(guest_tr_base, guest::TR_BASE);
    expose_field!(guest_gdtr_base, guest::GDTR_BASE);
    expose_field!(guest_idtr_base, guest::IDTR_BASE);
    expose_field!(guest_dr7, guest::DR7);
    expose_field!(guest_rsp, guest::RSP);
    expose_field!(guest_rip, guest::RIP);
    expose_field!(guest_rflags, guest::RFLAGS);
    expose_field!(guest_pending_dbg_exceptions, guest::PENDING_DBG_EXCEPTIONS);
    expose_field!(guest_sysenter_esp, guest::IA32_SYSENTER_ESP);
    expose_field!(guest_sysenter_eip, guest::IA32_SYSENTER_EIP);
    expose_field!(host_cr0, host::CR0);
    expose_field!(host_cr3, host::CR3);
    expose_field!(host_cr4, host::CR4);
    expose_field!(host_fs_base, host::FS_BASE);
    expose_field!(host_gs_base, host::GS_BASE);
    expose_field!(host_tr_base, host::TR_BASE);
    expose_field!(host_gdtr_base, host::GDTR_BASE);
    expose_field!(host_idtr_base, host::IDTR_BASE);
    expose_field!(host_ia32_sysenter_esp, host::IA32_SYSENTER_ESP);
    expose_field!(host_ia32_sysenter_eip, host::IA32_SYSENTER_EIP);
    expose_field!(host_rsp, host::RSP);
    expose_field!(host_rip, host::RIP);
}

impl Display for Vmcs {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "GUEST VM STATE")?;
        writeln!(
            f,
            "activity_state={:016x}",
            self.read_guest_activity_state().unwrap()
        )?;
        writeln!(f, "bndcfgs={:016x}", self.read_guest_bndcfgs().unwrap())?;
        writeln!(f, "cr0={:016x}", self.read_guest_cr0().unwrap())?;
        writeln!(f, "cr3={:016x}", self.read_guest_cr3().unwrap())?;
        writeln!(f, "cr4={:016x}", self.read_guest_cr4().unwrap())?;

        writeln!(f, "cs_ar={:016x}", self.read_guest_cs_ar_bytes().unwrap())?;
        writeln!(f, "cs_base={:016x}", self.read_guest_cs_base().unwrap())?;
        writeln!(f, "cs_limit={:016x}", self.read_guest_cs_limit().unwrap())?;
        writeln!(
            f,
            "cs_selector={:016x}",
            self.read_guest_cs_selector().unwrap()
        )?;

        writeln!(f, "ds_ar={:016x}", self.read_guest_ds_ar_bytes().unwrap())?;
        writeln!(f, "ds_base={:016x}", self.read_guest_ds_base().unwrap())?;
        writeln!(f, "ds_limit={:016x}", self.read_guest_ds_limit().unwrap())?;
        writeln!(f, "ds_sel={:016x}", self.read_guest_ds_selector().unwrap())?;

        writeln!(f, "es_ar={:016x}", self.read_guest_es_ar_bytes().unwrap())?;
        writeln!(f, "es_base={:016x}", self.read_guest_es_base().unwrap())?;
        writeln!(f, "es_limit={:016x}", self.read_guest_es_limit().unwrap())?;
        writeln!(
            f,
            "es_selector={:016x}",
            self.read_guest_es_selector().unwrap()
        )?;

        writeln!(f, "fs_ar={:016x}", self.read_guest_fs_ar_bytes().unwrap())?;
        writeln!(f, "fs_base={:016x}", self.read_guest_fs_base().unwrap())?;
        writeln!(f, "fs_limit={:016x}", self.read_guest_fs_limit().unwrap())?;
        writeln!(
            f,
            "fs_selector={:016x}",
            self.read_guest_fs_selector().unwrap()
        )?;

        writeln!(f, "gs_ar={:016x}", self.read_guest_gs_ar_bytes().unwrap())?;
        writeln!(f, "gs_base={:016x}", self.read_guest_gs_base().unwrap())?;
        writeln!(f, "gs_limit={:016x}", self.read_guest_gs_limit().unwrap())?;
        writeln!(
            f,
            "gs_selector={:016x}",
            self.read_guest_gs_selector().unwrap()
        )?;

        writeln!(f, "ss_ar={:016x}", self.read_guest_ss_ar_bytes().unwrap())?;
        writeln!(f, "ss_base={:016x}", self.read_guest_ss_base().unwrap())?;
        writeln!(f, "ss_limit={:016x}", self.read_guest_ss_limit().unwrap())?;
        writeln!(f, "ss_sel={:016x}", self.read_guest_ss_selector().unwrap())?;

        writeln!(f, "gdtr_base={:016x}", self.read_guest_gdtr_base().unwrap())?;
        writeln!(
            f,
            "gdtr_limit={:016x}",
            self.read_guest_gdtr_limit().unwrap()
        )?;

        writeln!(f, "idtr_base={:016x}", self.read_guest_idtr_base().unwrap())?;
        writeln!(
            f,
            "idtr_limit={:016x}",
            self.read_guest_idtr_limit().unwrap()
        )?;

        writeln!(f, "dr7={:016x}", self.read_guest_dr7().unwrap())?;

        writeln!(
            f,
            "ia32_debugctl={:016x}",
            self.read_guest_ia32_debugctl().unwrap()
        )?;
        writeln!(f, "ia32_efer={:016x}", self.read_guest_ia32_efer().unwrap())?;
        writeln!(f, "ia32_pat={:016x}", self.read_guest_ia32_pat().unwrap())?;
        writeln!(
            f,
            "ia32_perf_global_ctrl={:016x}",
            self.read_guest_ia32_perf_global_ctrl().unwrap()
        )?;
        // writeln!(
        //     f,
        //     "ia32_rtit_ctl={:016x}",
        //     self.read_guest_ia32_rtit_ctl().unwrap()
        // )?;
        writeln!(
            f,
            "intrpt_state={:016x}",
            self.read_guest_interruptibility_state().unwrap()
        )?;
        writeln!(
            f,
            "intr_status={:016x}",
            self.read_guest_intr_status().unwrap()
        )?;
        writeln!(
            f,
            "ldtr_ar={:016x}",
            self.read_guest_ldtr_ar_bytes().unwrap()
        )?;
        writeln!(f, "ldtr_base={:016x}", self.read_guest_ldtr_base().unwrap())?;
        writeln!(
            f,
            "ldtr_limit={:016x}",
            self.read_guest_ldtr_limit().unwrap()
        )?;
        writeln!(
            f,
            "ldtr_sel={:016x}",
            self.read_guest_ldtr_selector().unwrap()
        )?;
        writeln!(
            f,
            "guest_lin_addr={:016x}",
            self.read_guest_linear_address().unwrap()
        )?;
        writeln!(f, "pdptr0={:016x}", self.read_guest_pdptr0().unwrap())?;
        writeln!(f, "pdptr1={:016x}", self.read_guest_pdptr1().unwrap())?;
        writeln!(f, "pdptr2={:016x}", self.read_guest_pdptr2().unwrap())?;
        writeln!(f, "pdptr3={:016x}", self.read_guest_pdptr3().unwrap())?;
        writeln!(
            f,
            "pending_dbt_exceptions={:016x}",
            self.read_guest_pending_dbg_exceptions().unwrap()
        )?;
        writeln!(
            f,
            "guest_phys_addr={:016x}",
            self.read_guest_physical_address().unwrap()
        )?;
        writeln!(f, "pml_index={:016x}", self.read_guest_pml_index().unwrap())?;
        writeln!(f, "rflags={:016x}", self.read_guest_rflags().unwrap())?;
        writeln!(f, "rip={:016x}", self.read_guest_rip().unwrap())?;
        writeln!(f, "rsp={:016x}", self.read_guest_rsp().unwrap())?;

        writeln!(
            f,
            "systeneter_cs={:016x}",
            self.read_guest_sysenter_cs().unwrap()
        )?;
        writeln!(
            f,
            "sysenter_eip={:016x}",
            self.read_guest_sysenter_eip().unwrap()
        )?;
        writeln!(
            f,
            "sysenter_esp={:016x}",
            self.read_guest_sysenter_esp().unwrap()
        )?;
        writeln!(f, "tr_ar={:016x}", self.read_guest_tr_ar_bytes().unwrap())?;
        writeln!(f, "tr_base={:016x}", self.read_guest_tr_base().unwrap())?;
        writeln!(f, "tr_limit={:016x}", self.read_guest_tr_limit().unwrap())?;
        writeln!(
            f,
            "tr_selector={:016x}",
            self.read_guest_tr_selector().unwrap()
        )?;
        writeln!(
            f,
            "vmcs_link_ptr={:016x}",
            self.read_vmcs_link_pointer().unwrap()
        )?;

        Ok(())
    }
}
