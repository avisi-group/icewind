use {
    crate::arch::x86::memory::{BoxToVirtAddrExt, VirtAddrExt},
    alloc::boxed::Box,
    x86::{
        bits64::vmx::{vmclear, vmptrld, vmread, vmwrite},
        msr::{IA32_VMX_BASIC, rdmsr},
        vmx::vmcs::ro::{EXIT_REASON, VM_INSTRUCTION_ERROR},
    },
    x86_64::{PhysAddr, VirtAddr},
};

mod fields;

#[derive(PartialEq, Eq)]
enum VmcsState {
    Inactive,
    Active,
}

pub struct Vmcs {
    region: Box<VmcsRegion>,
    state: VmcsState,
    launched: bool,
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum VmcsError {
    /// VMCS was not active
    NotActive,
    /// Operation failed
    Failed,
}

impl Vmcs {
    /// Initializes the VMCS region.
    pub fn new() -> Self {
        // clear bit 31
        let revision_id = (unsafe { rdmsr(IA32_VMX_BASIC) } as u32) & !(1 << 31);

        let region = Box::new(VmcsRegion {
            revision_id,
            data: [0u8; _],
        });

        unsafe {
            vmclear(
                VirtAddr::from_ptr((&*region) as *const VmcsRegion)
                    .to_phys()
                    .as_u64(),
            )
            .unwrap();
        }

        Self {
            region,
            state: VmcsState::Inactive,
            launched: false,
        }
    }

    fn get_region_phys(&self) -> PhysAddr {
        self.region.as_virt().to_phys()
    }

    pub fn activate(&mut self) -> Result<(), VmcsError> {
        // vmptrld
        unsafe {
            vmptrld(self.get_region_phys().as_u64()).map_err(|_| VmcsError::Failed)?;
        }

        self.state = VmcsState::Active;
        Ok(())
    }

    pub fn deactivate(&mut self) -> Result<(), VmcsError> {
        self.ensure_active()?;

        unsafe {
            vmclear(self.get_region_phys().as_u64()).map_err(|_| VmcsError::Failed)?;
        }

        self.state = VmcsState::Inactive;

        Ok(())
    }

    pub fn is_launched(&self) -> bool {
        self.launched
    }

    pub fn set_launched(&mut self) {
        self.launched = true;
    }

    fn ensure_active(&self) -> Result<(), VmcsError> {
        if self.state != VmcsState::Active {
            Err(VmcsError::NotActive)
        } else {
            Ok(())
        }
    }

    fn get_field(&self, field: u32) -> Result<u64, VmcsError> {
        self.ensure_active()?;

        unsafe { vmread(field).map_err(|_| VmcsError::Failed) }
    }

    fn set_field(&self, field: u32, value: u64) -> Result<(), VmcsError> {
        self.ensure_active()?;

        unsafe { vmwrite(field, value).map_err(|_| VmcsError::Failed) }
    }

    pub fn read_vm_instruction_error(&self) -> Result<&'static str, VmcsError> {
        self.get_field(VM_INSTRUCTION_ERROR)
            .map(|i| INSTRUCTION_ERROR_MESSAGES[usize::try_from(i).unwrap()])
    }

    pub fn read_vm_exit_reason(&self) -> Result<VmxExitReason, VmcsError> {
        self.get_field(EXIT_REASON).map(|i| VmxExitReason::from(i))
    }
}

#[repr(C, align(4096))]
pub struct VmcsRegion {
    /// Revision ID required for VMXON.
    pub revision_id: u32,

    /// Data array constituting the rest of the VMXON region.
    pub data: [u8; 4096 - 4],
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum VmInstructionError {
    /// No error
    NoError,
    /// Vmcall executed in vmx root operation
    VmCallExecInVmxRootOp,
    /// Vmclear with invalid physical address
    VmClearInvalidPhysicalAddress,
    /// vmclear with vmxon pointer
    VmClearVmxOnPointerEqual,
    /// vmlaunch with non-clear vmcs
    VmLaunchNonClearVmcs,
    /// vmresume with non-launched vmcs
    VmresumeNonLaunchedVmcs,
    /// vmresume after vmxoff (vmxoff and vmxon between vmlaunch and vmresume)
    VmresumeAfterVmxoff,
    /// vm entry with invalid control field(s)
    VmEntryInvalidControlFields,
    /// vm entry with invalid host-state field(s)
    VmEntryInvalidHostStateFields,
    /// vmptrld with invalid physical address
    VmptrldInvalidPhysAddr,
    /// vmptrld with vmxon pointer
    VmptrldVmxonPtrEqual,
    /// vmptrld with incorrect vmcs revision identifier
    VmptrldIncorrectVmcsRevision,
    /// vmread/vmwrite from/to unsupported vmcs component
    VmreadwriteUnsupportedVmcsComponent,
    /// vmwrite to read-only vmcs component
    VmwriteOnReadonlyVmcsComponent,
    /// vmxon executed in vmx root operation
    VmxOnExecInVmxRootOp,
    /// vm entry with invalid executive-vmcs pointer
    VmEntryInvalidExecutiveVmcsPtr,
    /// vm entry with non-launched executive vmcs
    VmEntryNonlaunchedExecutiveVmcs,
    /// vm entry with executive-vmcs pointer not vmxon pointer
    VmentryExecutiveVmcsPtrNotVmxon,
}

const INSTRUCTION_ERROR_MESSAGES: &[&'static str] = &[
    "No error",
    "vmcall executed in vmx root operation",
    "vmclear with invalid physical address",
    "vmclear with vmxon pointer",
    "vmlaunch with non-clear vmcs",
    "vmresume with non-launched vmcs",
    "vmresume after vmxoff (vmxoff and vmxon between vmlaunch and vmresume)",
    "vm entry with invalid control field(s)",
    "vm entry with invalid host-state field(s)",
    "vmptrld with invalid physical address",
    "vmptrld with vmxon pointer",
    "vmptrld with incorrect vmcs revision identifier",
    "vmread/vmwrite from/to unsupported vmcs component",
    "vmwrite to read-only vmcs component",
    "vmxon executed in vmx root operation",
    "vm entry with invalid executive-vmcs pointer",
    "vm entry with non-launched executive vmcs",
    "vm entry with executive-vmcs pointer not vmxon pointer",
];

#[derive(Debug)]
pub enum VmxExitReason {
    Exception = 0,
    ExternalInterrupt = 1,
    TripleFault = 2,
    InitSignal = 3,
    StartupIpi = 4,
    SystemManagementInterrupt = 5,
    OtherSmi = 6,
    InterruptWindow = 7,
    NmiWindow = 8,
    TaskSwitch = 9,
    CpuId = 10,
    GetSec = 11,
    Hlt = 12,
    Invd = 13,
    InvlPg = 14,
    RdPmc = 15,
    RdTsc = 16,
    Rsm = 17,
    VmCall = 18,
    VmClear = 19,
    VmLaunch = 20,
    VmPtrLd = 21,
    VmPtrSt = 22,
    VmRead = 23,
    IoInstruction = 30,
    RdMsr = 31,
    WrMsr = 32,
    VmEntryFailureInvalidGuestState = 33,
    EptViolation = 48,
    VmxPreemptionTimerExpired = 52,
}
impl From<u64> for VmxExitReason {
    fn from(value: u64) -> Self {
        use VmxExitReason::*;
        match value {
            0 => Exception,
            1 => ExternalInterrupt,
            2 => TripleFault,
            3 => InitSignal,
            4 => StartupIpi,
            5 => SystemManagementInterrupt,
            6 => OtherSmi,
            7 => InterruptWindow,
            8 => NmiWindow,
            9 => TaskSwitch,
            10 => CpuId,
            11 => GetSec,
            12 => Hlt,
            13 => Invd,
            14 => InvlPg,
            15 => RdPmc,
            16 => RdTsc,
            17 => Rsm,
            18 => VmCall,
            19 => VmClear,
            20 => VmLaunch,
            21 => VmPtrLd,
            22 => VmPtrSt,
            23 => VmRead,
            30 => IoInstruction,
            31 => RdMsr,
            32 => WrMsr,
            33 => VmEntryFailureInvalidGuestState,
            48 => EptViolation,
            52 => VmxPreemptionTimerExpired,
            _ => panic!("unknown exit reason"),
        }
    }
}
