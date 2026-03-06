use {
    common::ktest,
    core::fmt::{Display, Formatter},
    displaydoc::Display,
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, AsmRegisterXmm,
    },
    strum::{EnumCount, EnumIter, IntoEnumIterator},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, Hash)]
pub enum SegmentRegister {
    /// fs
    FS,
    /// gs
    GS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumCount, EnumIter)]
#[repr(u8)]
pub enum PhysicalRegister {
    /// rax
    RAX,
    /// rcx
    RCX,
    /// rdx
    RDX,
    /// rbx
    RBX,
    /// rsi
    RSI,
    /// rdi
    RDI,
    /// rsp
    RSP,
    /// rbp
    RBP,
    /// r8
    R8,
    /// r9
    R9,
    /// r10
    R10,
    /// r11
    R11,
    /// r12
    R12,
    /// r13
    R13,
    /// r14
    R14,
    /// r15
    R15,
    /// xmm0
    XMM0,
    /// xmm1
    XMM1,
    /// xmm2
    XMM2,
    /// xmm3
    XMM3,
    /// xmm4
    XMM4,
    /// xmm5
    XMM5,
    /// xmm6
    XMM6,
    /// xmm7
    XMM7,
}

impl PhysicalRegister {
    pub fn index(&self) -> usize {
        usize::from(*self as u8)
    }

    pub fn from_index(index: usize) -> Self {
        match index {
            0 => Self::RAX,
            1 => Self::RCX,
            2 => Self::RDX,
            3 => Self::RBX,
            4 => Self::RSI,
            5 => Self::RDI,
            6 => Self::RSP,
            7 => Self::RBP,
            8 => Self::R8,
            9 => Self::R9,
            10 => Self::R10,
            11 => Self::R11,
            12 => Self::R12,
            13 => Self::R13,
            14 => Self::R14,
            15 => Self::R15,
            16 => Self::XMM0,
            17 => Self::XMM1,
            18 => Self::XMM2,
            19 => Self::XMM3,
            20 => Self::XMM4,
            21 => Self::XMM5,
            22 => Self::XMM6,
            23 => Self::XMM7,
            _ => panic!(),
        }
    }

    pub fn class(&self) -> RegisterClass {
        if self.is_gpr() {
            RegisterClass::General
        } else {
            RegisterClass::Xmm
        }
    }

    pub fn is_gpr(&self) -> bool {
        !self.is_xmm()
    }

    pub fn is_xmm(&self) -> bool {
        matches!(
            self,
            Self::XMM0
                | Self::XMM1
                | Self::XMM2
                | Self::XMM3
                | Self::XMM4
                | Self::XMM5
                | Self::XMM6
                | Self::XMM7
        )
    }
}

impl TryFrom<&PhysicalRegister> for AsmRegister64 {
    type Error = RegisterConversionError;

    fn try_from(phys: &PhysicalRegister) -> Result<Self, Self::Error> {
        use iced_x86::code_asm::{
            r8, r9, r10, r11, r12, r13, r14, r15, rax, rbp, rbx, rcx, rdi, rdx, rsi, rsp,
        };

        match phys {
            PhysicalRegister::RAX => Ok(rax),
            PhysicalRegister::RCX => Ok(rcx),
            PhysicalRegister::RDX => Ok(rdx),
            PhysicalRegister::RBX => Ok(rbx),
            PhysicalRegister::RSI => Ok(rsi),
            PhysicalRegister::RDI => Ok(rdi),
            PhysicalRegister::RSP => Ok(rsp),
            PhysicalRegister::RBP => Ok(rbp),
            PhysicalRegister::R8 => Ok(r8),
            PhysicalRegister::R9 => Ok(r9),
            PhysicalRegister::R10 => Ok(r10),
            PhysicalRegister::R11 => Ok(r11),
            PhysicalRegister::R12 => Ok(r12),
            PhysicalRegister::R13 => Ok(r13),
            PhysicalRegister::R14 => Ok(r14),
            PhysicalRegister::R15 => Ok(r15),
            r => Err(RegisterConversionError::InvalidXmmAs64(*r)),
        }
    }
}

impl TryFrom<PhysicalRegister> for AsmRegister64 {
    type Error = RegisterConversionError;

    fn try_from(phys: PhysicalRegister) -> Result<Self, Self::Error> {
        Self::try_from(&phys)
    }
}

impl TryFrom<&PhysicalRegister> for AsmRegister8 {
    type Error = RegisterConversionError;

    fn try_from(phys: &PhysicalRegister) -> Result<Self, Self::Error> {
        use iced_x86::code_asm::{
            al, bl, bpl, cl, dil, dl, r8b, r9b, r10b, r11b, r12b, r13b, r14b, r15b, sil, spl,
        };

        match phys {
            PhysicalRegister::RAX => Ok(al),
            PhysicalRegister::RCX => Ok(cl),
            PhysicalRegister::RDX => Ok(dl),
            PhysicalRegister::RBX => Ok(bl),
            PhysicalRegister::RSI => Ok(sil),
            PhysicalRegister::RDI => Ok(dil),
            PhysicalRegister::RSP => Ok(spl),
            PhysicalRegister::RBP => Ok(bpl),
            PhysicalRegister::R8 => Ok(r8b),
            PhysicalRegister::R9 => Ok(r9b),
            PhysicalRegister::R10 => Ok(r10b),
            PhysicalRegister::R11 => Ok(r11b),
            PhysicalRegister::R12 => Ok(r12b),
            PhysicalRegister::R13 => Ok(r13b),
            PhysicalRegister::R14 => Ok(r14b),
            PhysicalRegister::R15 => Ok(r15b),
            _ => Err(RegisterConversionError::InvalidXmmAs8(*phys)),
        }
    }
}

impl TryFrom<PhysicalRegister> for AsmRegister8 {
    type Error = RegisterConversionError;

    fn try_from(phys: PhysicalRegister) -> Result<Self, Self::Error> {
        Self::try_from(&phys)
    }
}

impl TryFrom<&PhysicalRegister> for AsmRegister16 {
    type Error = RegisterConversionError;

    fn try_from(phys: &PhysicalRegister) -> Result<Self, Self::Error> {
        use iced_x86::code_asm::{
            ax, bp, bx, cx, di, dx, r8w, r9w, r10w, r11w, r12w, r13w, r14w, r15w, si, sp,
        };

        match phys {
            PhysicalRegister::RAX => Ok(ax),
            PhysicalRegister::RCX => Ok(cx),
            PhysicalRegister::RDX => Ok(dx),
            PhysicalRegister::RBX => Ok(bx),
            PhysicalRegister::RSI => Ok(si),
            PhysicalRegister::RDI => Ok(di),
            PhysicalRegister::RSP => Ok(sp),
            PhysicalRegister::RBP => Ok(bp),
            PhysicalRegister::R8 => Ok(r8w),
            PhysicalRegister::R9 => Ok(r9w),
            PhysicalRegister::R10 => Ok(r10w),
            PhysicalRegister::R11 => Ok(r11w),
            PhysicalRegister::R12 => Ok(r12w),
            PhysicalRegister::R13 => Ok(r13w),
            PhysicalRegister::R14 => Ok(r14w),
            PhysicalRegister::R15 => Ok(r15w),
            _ => Err(RegisterConversionError::InvalidXmmAs16(*phys)),
        }
    }
}

impl TryFrom<&PhysicalRegister> for AsmRegister32 {
    type Error = RegisterConversionError;

    fn try_from(phys: &PhysicalRegister) -> Result<Self, Self::Error> {
        use iced_x86::code_asm::{
            eax, ebp, ebx, ecx, edi, edx, esi, esp, r8d, r9d, r10d, r11d, r12d, r13d, r14d, r15d,
        };

        match phys {
            PhysicalRegister::RAX => Ok(eax),
            PhysicalRegister::RCX => Ok(ecx),
            PhysicalRegister::RDX => Ok(edx),
            PhysicalRegister::RBX => Ok(ebx),
            PhysicalRegister::RSI => Ok(esi),
            PhysicalRegister::RDI => Ok(edi),
            PhysicalRegister::RSP => Ok(esp),
            PhysicalRegister::RBP => Ok(ebp),
            PhysicalRegister::R8 => Ok(r8d),
            PhysicalRegister::R9 => Ok(r9d),
            PhysicalRegister::R10 => Ok(r10d),
            PhysicalRegister::R11 => Ok(r11d),
            PhysicalRegister::R12 => Ok(r12d),
            PhysicalRegister::R13 => Ok(r13d),
            PhysicalRegister::R14 => Ok(r14d),
            PhysicalRegister::R15 => Ok(r15d),
            _ => Err(RegisterConversionError::InvalidXmmAs32(*phys)),
        }
    }
}

impl TryFrom<PhysicalRegister> for AsmRegister16 {
    type Error = RegisterConversionError;
    fn try_from(phys: PhysicalRegister) -> Result<Self, Self::Error> {
        Self::try_from(&phys)
    }
}

impl TryFrom<PhysicalRegister> for AsmRegister32 {
    type Error = RegisterConversionError;

    fn try_from(phys: PhysicalRegister) -> Result<Self, Self::Error> {
        Self::try_from(&phys)
    }
}

impl TryFrom<&PhysicalRegister> for AsmRegisterXmm {
    type Error = RegisterConversionError;

    fn try_from(phys: &PhysicalRegister) -> Result<Self, Self::Error> {
        use iced_x86::code_asm::{xmm0, xmm1, xmm2, xmm3, xmm4, xmm5, xmm6, xmm7};

        match phys {
            PhysicalRegister::XMM0 => Ok(xmm0),
            PhysicalRegister::XMM1 => Ok(xmm1),
            PhysicalRegister::XMM2 => Ok(xmm2),
            PhysicalRegister::XMM3 => Ok(xmm3),
            PhysicalRegister::XMM4 => Ok(xmm4),
            PhysicalRegister::XMM5 => Ok(xmm5),
            PhysicalRegister::XMM6 => Ok(xmm6),
            PhysicalRegister::XMM7 => Ok(xmm7),
            reg => Err(RegisterConversionError::InvalidGeneralAsXmm(*reg)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Register {
    Physical(PhysicalRegister),
    Virtual { index: usize, class: RegisterClass },
    Global(usize),
}

impl Register {
    pub fn class(&self) -> Option<RegisterClass> {
        match self {
            Register::Global(_) => None,
            Register::Virtual { class, .. } => Some(*class),
            Register::Physical(phys) => Some(phys.class()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegisterClass {
    General,
    Xmm,
}

impl Display for RegisterClass {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            RegisterClass::General => write!(f, "gpr"),
            RegisterClass::Xmm => write!(f, "xmm"),
        }
    }
}

impl Display for Register {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Register::Physical(pr) => write!(f, "%{pr}"),
            Register::Virtual {
                index,
                class: RegisterClass::General,
            } => write!(f, "vg{index}"),
            Register::Virtual {
                index,
                class: RegisterClass::Xmm,
            } => write!(f, "vx{index}"),
            Register::Global(gr) => write!(f, "g{gr}"),
        }
    }
}

impl Into<iced_x86::Register> for PhysicalRegister {
    fn into(self) -> iced_x86::Register {
        match self {
            PhysicalRegister::RAX => iced_x86::Register::RAX,
            _ => todo!(),
        }
    }
}

#[derive(Debug, Clone, Copy, displaydoc::Display, thiserror::Error)]
pub enum RegisterConversionError {
    /// XMM register {0} cannot be converted to `AsmRegister8`
    InvalidXmmAs8(PhysicalRegister),
    /// XMM register {0} cannot be converted to `AsmRegister16`
    InvalidXmmAs16(PhysicalRegister),
    /// XMM register {0} cannot be converted to `AsmRegister32`
    InvalidXmmAs32(PhysicalRegister),
    /// XMM register {0} cannot be converted to `AsmRegister64`
    InvalidXmmAs64(PhysicalRegister),
    /// General register {0} cannot be converted to `AsmRegisterXmm`
    InvalidGeneralAsXmm(PhysicalRegister),
}

#[ktest]
fn reg_index() {
    for i in 0..PhysicalRegister::COUNT {
        assert_eq!(i, PhysicalRegister::from_index(i).index())
    }

    for reg in PhysicalRegister::iter() {
        assert_eq!(reg, PhysicalRegister::from_index(reg.index()))
    }
}
