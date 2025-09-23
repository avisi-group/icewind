use {
    core::fmt::{Display, Formatter},
    displaydoc::Display,
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, AsmRegisterXmm,
    },
    proc_macro_lib::ktest,
    strum::{EnumCount, EnumIter, IntoEnumIterator},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
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

impl From<&PhysicalRegister> for AsmRegister64 {
    fn from(phys: &PhysicalRegister) -> Self {
        use iced_x86::code_asm::{
            r8, r9, r10, r11, r12, r13, r14, r15, rax, rbp, rbx, rcx, rdi, rdx, rsi, rsp,
        };

        match phys {
            PhysicalRegister::RAX => rax,
            PhysicalRegister::RCX => rcx,
            PhysicalRegister::RDX => rdx,
            PhysicalRegister::RBX => rbx,
            PhysicalRegister::RSI => rsi,
            PhysicalRegister::RDI => rdi,
            PhysicalRegister::RSP => rsp,
            PhysicalRegister::RBP => rbp,
            PhysicalRegister::R8 => r8,
            PhysicalRegister::R9 => r9,
            PhysicalRegister::R10 => r10,
            PhysicalRegister::R11 => r11,
            PhysicalRegister::R12 => r12,
            PhysicalRegister::R13 => r13,
            PhysicalRegister::R14 => r14,
            PhysicalRegister::R15 => r15,
            _ => panic!(),
        }
    }
}

impl From<PhysicalRegister> for AsmRegister64 {
    fn from(phys: PhysicalRegister) -> Self {
        Self::from(&phys)
    }
}

impl From<&PhysicalRegister> for AsmRegister8 {
    fn from(phys: &PhysicalRegister) -> Self {
        use iced_x86::code_asm::{
            al, bl, bpl, cl, dil, dl, r8b, r9b, r10b, r11b, r12b, r13b, r14b, r15b, sil, spl,
        };

        match phys {
            PhysicalRegister::RAX => al,
            PhysicalRegister::RCX => cl,
            PhysicalRegister::RDX => dl,
            PhysicalRegister::RBX => bl,
            PhysicalRegister::RSI => sil,
            PhysicalRegister::RDI => dil,
            PhysicalRegister::RSP => spl,
            PhysicalRegister::RBP => bpl,
            PhysicalRegister::R8 => r8b,
            PhysicalRegister::R9 => r9b,
            PhysicalRegister::R10 => r10b,
            PhysicalRegister::R11 => r11b,
            PhysicalRegister::R12 => r12b,
            PhysicalRegister::R13 => r13b,
            PhysicalRegister::R14 => r14b,
            PhysicalRegister::R15 => r15b,
            _ => panic!(),
        }
    }
}

impl From<PhysicalRegister> for AsmRegister8 {
    fn from(phys: PhysicalRegister) -> Self {
        Self::from(&phys)
    }
}

impl From<&PhysicalRegister> for AsmRegister16 {
    fn from(phys: &PhysicalRegister) -> Self {
        use iced_x86::code_asm::{
            ax, bp, bx, cx, di, dx, r8w, r9w, r10w, r11w, r12w, r13w, r14w, r15w, si, sp,
        };

        match phys {
            PhysicalRegister::RAX => ax,
            PhysicalRegister::RCX => cx,
            PhysicalRegister::RDX => dx,
            PhysicalRegister::RBX => bx,
            PhysicalRegister::RSI => si,
            PhysicalRegister::RDI => di,
            PhysicalRegister::RSP => sp,
            PhysicalRegister::RBP => bp,
            PhysicalRegister::R8 => r8w,
            PhysicalRegister::R9 => r9w,
            PhysicalRegister::R10 => r10w,
            PhysicalRegister::R11 => r11w,
            PhysicalRegister::R12 => r12w,
            PhysicalRegister::R13 => r13w,
            PhysicalRegister::R14 => r14w,
            PhysicalRegister::R15 => r15w,
            _ => panic!(),
        }
    }
}

impl From<&PhysicalRegister> for AsmRegister32 {
    fn from(phys: &PhysicalRegister) -> Self {
        use iced_x86::code_asm::{
            eax, ebp, ebx, ecx, edi, edx, esi, esp, r8d, r9d, r10d, r11d, r12d, r13d, r14d, r15d,
        };

        match phys {
            PhysicalRegister::RAX => eax,
            PhysicalRegister::RCX => ecx,
            PhysicalRegister::RDX => edx,
            PhysicalRegister::RBX => ebx,
            PhysicalRegister::RSI => esi,
            PhysicalRegister::RDI => edi,
            PhysicalRegister::RSP => esp,
            PhysicalRegister::RBP => ebp,
            PhysicalRegister::R8 => r8d,
            PhysicalRegister::R9 => r9d,
            PhysicalRegister::R10 => r10d,
            PhysicalRegister::R11 => r11d,
            PhysicalRegister::R12 => r12d,
            PhysicalRegister::R13 => r13d,
            PhysicalRegister::R14 => r14d,
            PhysicalRegister::R15 => r15d,
            _ => panic!(),
        }
    }
}

impl From<PhysicalRegister> for AsmRegister16 {
    fn from(phys: PhysicalRegister) -> Self {
        Self::from(&phys)
    }
}

impl From<PhysicalRegister> for AsmRegister32 {
    fn from(phys: PhysicalRegister) -> Self {
        Self::from(&phys)
    }
}

impl From<&PhysicalRegister> for AsmRegisterXmm {
    fn from(phys: &PhysicalRegister) -> Self {
        use iced_x86::code_asm::{xmm0, xmm1, xmm2, xmm3, xmm4, xmm5, xmm6, xmm7};

        match phys {
            PhysicalRegister::XMM0 => xmm0,
            PhysicalRegister::XMM1 => xmm1,
            PhysicalRegister::XMM2 => xmm2,
            PhysicalRegister::XMM3 => xmm3,
            PhysicalRegister::XMM4 => xmm4,
            PhysicalRegister::XMM5 => xmm5,
            PhysicalRegister::XMM6 => xmm6,
            PhysicalRegister::XMM7 => xmm7,
            _ => panic!(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Register {
    Physical(PhysicalRegister),
    Virtual(usize),
    Global(usize),
}

impl Display for Register {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Register::Physical(pr) => write!(f, "%{pr}"),
            Register::Virtual(vr) => write!(f, "v{vr}"),
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
enum RegisterConversionError {
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
