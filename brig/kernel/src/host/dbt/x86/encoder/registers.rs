use {
    core::fmt::{Display, Formatter},
    displaydoc::Display,
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, AsmRegisterXmm,
    },
    strum::{EnumCount, EnumIter},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, Hash, EnumIter, EnumCount)]
#[repr(u32)]
pub enum PhysicalRegisterGeneral {
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
}

impl From<&PhysicalRegisterGeneral> for AsmRegister64 {
    fn from(phys: &PhysicalRegisterGeneral) -> Self {
        use iced_x86::code_asm::{
            r8, r9, r10, r11, r12, r13, r14, r15, rax, rbp, rbx, rcx, rdi, rdx, rsi, rsp,
        };

        match phys {
            PhysicalRegisterGeneral::RAX => rax,
            PhysicalRegisterGeneral::RCX => rcx,
            PhysicalRegisterGeneral::RDX => rdx,
            PhysicalRegisterGeneral::RBX => rbx,
            PhysicalRegisterGeneral::RSI => rsi,
            PhysicalRegisterGeneral::RDI => rdi,
            PhysicalRegisterGeneral::RSP => rsp,
            PhysicalRegisterGeneral::RBP => rbp,
            PhysicalRegisterGeneral::R8 => r8,
            PhysicalRegisterGeneral::R9 => r9,
            PhysicalRegisterGeneral::R10 => r10,
            PhysicalRegisterGeneral::R11 => r11,
            PhysicalRegisterGeneral::R12 => r12,
            PhysicalRegisterGeneral::R13 => r13,
            PhysicalRegisterGeneral::R14 => r14,
            PhysicalRegisterGeneral::R15 => r15,
        }
    }
}

impl From<PhysicalRegisterGeneral> for AsmRegister64 {
    fn from(phys: PhysicalRegisterGeneral) -> Self {
        Self::from(&phys)
    }
}

impl From<&PhysicalRegisterGeneral> for AsmRegister8 {
    fn from(phys: &PhysicalRegisterGeneral) -> Self {
        use iced_x86::code_asm::{
            al, bl, bpl, cl, dil, dl, r8b, r9b, r10b, r11b, r12b, r13b, r14b, r15b, sil, spl,
        };

        match phys {
            PhysicalRegisterGeneral::RAX => al,
            PhysicalRegisterGeneral::RCX => cl,
            PhysicalRegisterGeneral::RDX => dl,
            PhysicalRegisterGeneral::RBX => bl,
            PhysicalRegisterGeneral::RSI => sil,
            PhysicalRegisterGeneral::RDI => dil,
            PhysicalRegisterGeneral::RSP => spl,
            PhysicalRegisterGeneral::RBP => bpl,
            PhysicalRegisterGeneral::R8 => r8b,
            PhysicalRegisterGeneral::R9 => r9b,
            PhysicalRegisterGeneral::R10 => r10b,
            PhysicalRegisterGeneral::R11 => r11b,
            PhysicalRegisterGeneral::R12 => r12b,
            PhysicalRegisterGeneral::R13 => r13b,
            PhysicalRegisterGeneral::R14 => r14b,
            PhysicalRegisterGeneral::R15 => r15b,
        }
    }
}

impl From<PhysicalRegisterGeneral> for AsmRegister8 {
    fn from(phys: PhysicalRegisterGeneral) -> Self {
        Self::from(&phys)
    }
}

impl From<&PhysicalRegisterGeneral> for AsmRegister16 {
    fn from(phys: &PhysicalRegisterGeneral) -> Self {
        use iced_x86::code_asm::{
            ax, bp, bx, cx, di, dx, r8w, r9w, r10w, r11w, r12w, r13w, r14w, r15w, si, sp,
        };

        match phys {
            PhysicalRegisterGeneral::RAX => ax,
            PhysicalRegisterGeneral::RCX => cx,
            PhysicalRegisterGeneral::RDX => dx,
            PhysicalRegisterGeneral::RBX => bx,
            PhysicalRegisterGeneral::RSI => si,
            PhysicalRegisterGeneral::RDI => di,
            PhysicalRegisterGeneral::RSP => sp,
            PhysicalRegisterGeneral::RBP => bp,
            PhysicalRegisterGeneral::R8 => r8w,
            PhysicalRegisterGeneral::R9 => r9w,
            PhysicalRegisterGeneral::R10 => r10w,
            PhysicalRegisterGeneral::R11 => r11w,
            PhysicalRegisterGeneral::R12 => r12w,
            PhysicalRegisterGeneral::R13 => r13w,
            PhysicalRegisterGeneral::R14 => r14w,
            PhysicalRegisterGeneral::R15 => r15w,
        }
    }
}

impl From<&PhysicalRegisterGeneral> for AsmRegister32 {
    fn from(phys: &PhysicalRegisterGeneral) -> Self {
        use iced_x86::code_asm::{
            eax, ebp, ebx, ecx, edi, edx, esi, esp, r8d, r9d, r10d, r11d, r12d, r13d, r14d, r15d,
        };

        match phys {
            PhysicalRegisterGeneral::RAX => eax,
            PhysicalRegisterGeneral::RCX => ecx,
            PhysicalRegisterGeneral::RDX => edx,
            PhysicalRegisterGeneral::RBX => ebx,
            PhysicalRegisterGeneral::RSI => esi,
            PhysicalRegisterGeneral::RDI => edi,
            PhysicalRegisterGeneral::RSP => esp,
            PhysicalRegisterGeneral::RBP => ebp,
            PhysicalRegisterGeneral::R8 => r8d,
            PhysicalRegisterGeneral::R9 => r9d,
            PhysicalRegisterGeneral::R10 => r10d,
            PhysicalRegisterGeneral::R11 => r11d,
            PhysicalRegisterGeneral::R12 => r12d,
            PhysicalRegisterGeneral::R13 => r13d,
            PhysicalRegisterGeneral::R14 => r14d,
            PhysicalRegisterGeneral::R15 => r15d,
        }
    }
}

impl From<PhysicalRegisterGeneral> for AsmRegister16 {
    fn from(phys: PhysicalRegisterGeneral) -> Self {
        Self::from(&phys)
    }
}

impl From<PhysicalRegisterGeneral> for AsmRegister32 {
    fn from(phys: PhysicalRegisterGeneral) -> Self {
        Self::from(&phys)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, Hash, EnumIter, EnumCount)]
#[repr(u32)]
pub enum PhysicalRegisterXmm {
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

impl From<&PhysicalRegisterXmm> for AsmRegisterXmm {
    fn from(phys: &PhysicalRegisterXmm) -> Self {
        use iced_x86::code_asm::{xmm0, xmm1, xmm2, xmm3, xmm4, xmm5, xmm6, xmm7};

        match phys {
            PhysicalRegisterXmm::XMM0 => xmm0,
            PhysicalRegisterXmm::XMM1 => xmm1,
            PhysicalRegisterXmm::XMM2 => xmm2,
            PhysicalRegisterXmm::XMM3 => xmm3,
            PhysicalRegisterXmm::XMM4 => xmm4,
            PhysicalRegisterXmm::XMM5 => xmm5,
            PhysicalRegisterXmm::XMM6 => xmm6,
            PhysicalRegisterXmm::XMM7 => xmm7,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum SegmentRegister {
    /// fs
    FS,
    /// gs
    GS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
pub enum PhysicalRegister {
    /// {0}
    General(PhysicalRegisterGeneral),
    /// {0}
    Xmm(PhysicalRegisterXmm),
}

// todo: do this with a strum macro probably
impl PhysicalRegister {
    pub const RAX: Self = Self::General(PhysicalRegisterGeneral::RAX);
    pub const RCX: Self = Self::General(PhysicalRegisterGeneral::RCX);
    pub const RDX: Self = Self::General(PhysicalRegisterGeneral::RDX);
    pub const RBX: Self = Self::General(PhysicalRegisterGeneral::RBX);
    pub const RSI: Self = Self::General(PhysicalRegisterGeneral::RSI);
    pub const RDI: Self = Self::General(PhysicalRegisterGeneral::RDI);
    pub const RSP: Self = Self::General(PhysicalRegisterGeneral::RSP);
    pub const RBP: Self = Self::General(PhysicalRegisterGeneral::RBP);
    pub const R8: Self = Self::General(PhysicalRegisterGeneral::R8);
    pub const R9: Self = Self::General(PhysicalRegisterGeneral::R9);
    pub const R10: Self = Self::General(PhysicalRegisterGeneral::R10);
    pub const R11: Self = Self::General(PhysicalRegisterGeneral::R11);
    pub const R12: Self = Self::General(PhysicalRegisterGeneral::R12);
    pub const R13: Self = Self::General(PhysicalRegisterGeneral::R13);
    pub const R14: Self = Self::General(PhysicalRegisterGeneral::R14);
    pub const R15: Self = Self::General(PhysicalRegisterGeneral::R15);

    pub const XMM0: Self = Self::Xmm(PhysicalRegisterXmm::XMM0);
    pub const XMM1: Self = Self::Xmm(PhysicalRegisterXmm::XMM1);
    pub const XMM2: Self = Self::Xmm(PhysicalRegisterXmm::XMM2);
    pub const XMM3: Self = Self::Xmm(PhysicalRegisterXmm::XMM3);
    pub const XMM4: Self = Self::Xmm(PhysicalRegisterXmm::XMM4);
    pub const XMM5: Self = Self::Xmm(PhysicalRegisterXmm::XMM5);
    pub const XMM6: Self = Self::Xmm(PhysicalRegisterXmm::XMM6);
    pub const XMM7: Self = Self::Xmm(PhysicalRegisterXmm::XMM7);

    pub fn index(&self) -> usize {
        match self {
            PhysicalRegister::General(physical_register_general) => {
                (*physical_register_general as u32) as usize
            }
            PhysicalRegister::Xmm(physical_register_xmm) => {
                (*physical_register_xmm as u32) as usize + PhysicalRegisterGeneral::COUNT
            }
        }
    }

    pub fn is_gpr(&self) -> bool {
        match self {
            PhysicalRegister::General(_) => true,
            PhysicalRegister::Xmm(_) => false,
        }
    }

    pub fn is_xmm(&self) -> bool {
        match self {
            PhysicalRegister::General(_) => true,
            PhysicalRegister::Xmm(_) => false,
        }
    }
}

impl From<&PhysicalRegister> for AsmRegister8 {
    fn from(phys: &PhysicalRegister) -> Self {
        let PhysicalRegister::General(g) = phys else {
            panic!()
        };
        Self::from(g)
    }
}

impl From<PhysicalRegister> for AsmRegister8 {
    fn from(phys: PhysicalRegister) -> Self {
        Self::from(&phys)
    }
}

impl From<&PhysicalRegister> for AsmRegister16 {
    fn from(phys: &PhysicalRegister) -> Self {
        let PhysicalRegister::General(g) = phys else {
            panic!()
        };
        Self::from(g)
    }
}

impl From<PhysicalRegister> for AsmRegister16 {
    fn from(phys: PhysicalRegister) -> Self {
        Self::from(&phys)
    }
}

impl From<&PhysicalRegister> for AsmRegister32 {
    fn from(phys: &PhysicalRegister) -> Self {
        let PhysicalRegister::General(g) = phys else {
            panic!()
        };
        Self::from(g)
    }
}

impl From<PhysicalRegister> for AsmRegister32 {
    fn from(phys: PhysicalRegister) -> Self {
        Self::from(&phys)
    }
}

impl From<&PhysicalRegister> for AsmRegister64 {
    fn from(phys: &PhysicalRegister) -> Self {
        let PhysicalRegister::General(g) = phys else {
            panic!()
        };
        Self::from(g)
    }
}

impl From<PhysicalRegister> for AsmRegister64 {
    fn from(phys: PhysicalRegister) -> Self {
        Self::from(&phys)
    }
}

impl From<&PhysicalRegister> for AsmRegisterXmm {
    fn from(phys: &PhysicalRegister) -> Self {
        let PhysicalRegister::Xmm(xmm) = phys else {
            panic!()
        };
        Self::from(xmm)
    }
}

impl From<PhysicalRegister> for AsmRegisterXmm {
    fn from(phys: PhysicalRegister) -> Self {
        Self::from(&phys)
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

impl Into<iced_x86::Register> for PhysicalRegisterGeneral {
    fn into(self) -> iced_x86::Register {
        match self {
            PhysicalRegisterGeneral::RAX => iced_x86::Register::RAX,
            _ => todo!(),
        }
    }
}

#[derive(Debug, Clone, Copy, displaydoc::Display, thiserror::Error)]
enum RegisterConversionError {
    /// XMM register {0} cannot be converted to `AsmRegister64`
    InvalidXmmAs64(PhysicalRegisterXmm),
    /// General register {0} cannot be converted to `AsmRegisterXmm`
    InvalidGeneralAsXmm(PhysicalRegisterGeneral),
}
