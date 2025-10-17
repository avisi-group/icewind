use {
    crate::{
        bump_alloc::BumpAllocatorRef,
        x86::{
            ARG_REGS, X86Block,
            encoder::{
                instructions::{
                    adc, add, and, cmp, jne, lea, mov, movsx, movzx, not, or, setne, shl, shr, sub,
                    test, xor,
                },
                registers::{PhysicalRegister, Register, SegmentRegister},
                width::Width,
            },
        },
    },
    alloc::vec::Vec,
    common::{arena::Ref, hashmap::HashMapA},
    core::fmt::{Debug, Display, Formatter},
    displaydoc::Display,
    iced_x86::code_asm::{
        AsmMemoryOperand, AsmRegister8, AsmRegister32, AsmRegister64, AsmRegisterXmm,
        CodeAssembler, CodeLabel, qword_ptr,
    },
};

mod instructions;
pub mod registers;
pub mod width;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum Opcode {
    /// mov {0}, {1}
    MOV(Operand, Operand),
    /// movzx {0}, {1}
    MOVZX(Operand, Operand),
    /// movsx {0}, {1}
    MOVSX(Operand, Operand),
    /// cmove {0}, {1}
    CMOVE(Operand, Operand),
    /// cmovne {0}, {1}
    CMOVNE(Operand, Operand),

    /// lea {0}, {1}
    LEA(Operand, Operand),
    /// shl {0}, {1}
    SHL(Operand, Operand),
    /// shr {0}, {1}
    SHR(Operand, Operand),
    /// shld {0}, {1}, {2}
    SHLD(Operand, Operand, Operand),
    /// shrd {0}, {1}, {2}
    SHRD(Operand, Operand, Operand),
    /// sar {0}, {1}
    SAR(Operand, Operand),
    /// add {0}, {1}
    ADD(Operand, Operand),
    /// adc {0}, {1}, {2}
    ADC(Operand, Operand, Operand),
    /// sub {0}, {1}
    SUB(Operand, Operand),
    /// or {0}, {1},
    OR(Operand, Operand),
    /// xor {0}, {1},
    XOR(Operand, Operand),
    /// and {0}, {1},
    AND(Operand, Operand),
    /// mul {0}, {1}, {2},
    MUL(Operand, Operand, Operand),
    /// imul {0}, {1},
    IMUL(Operand, Operand),
    /// imul1 {0}, {1}, {2} (one-operand form has 128 bit output)
    IMUL1(Operand, Operand, Operand),
    /// idiv RDX:RAX {0}
    IDIV(Operand),
    /// div RDX:RAX {0}
    DIV(Operand),
    /// cqo
    CQO,
    /// not {0}
    NOT(Operand),
    /// neg {0}
    NEG(Operand),
    /// bextr {0}, {1}, {2}
    BEXTR(Operand, Operand, Operand),
    /// pextr {0}, {1}, {2}
    PEXTR(Operand, Operand, Operand),
    /// pinsr {0}, {1}, {2}
    PINSR(Operand, Operand, Operand),
    /// punpckl {0}, {1}
    PUNPCKL(Operand, Operand),
    /// jmp {0}
    JMP(Operand),
    /// push {0}
    PUSH(Operand),
    /// pop {0}
    POP(Operand),
    /// ret
    RET,
    /// test {0}, {1}
    TEST(Operand, Operand),
    /// cmp {0}, {1}
    CMP(Operand, Operand),

    /// sets {0}
    SETS(Operand), //n
    /// sete {0}
    SETE(Operand), //z
    /// setc {0}
    SETC(Operand), //c
    /// seto {0}
    SETO(Operand), //v

    /// setne {0}
    SETNE(Operand),
    /// setnz {0}
    SETNZ(Operand),
    /// setb {0}
    SETB(Operand),
    /// setbe {0}
    SETBE(Operand),
    /// seta {0}
    SETA(Operand),
    /// setg {0}
    SETG(Operand),
    /// setge {0}
    SETGE(Operand),
    /// setl {0}
    SETL(Operand),
    /// setle {0}
    SETLE(Operand),
    /// setae {0}
    SETAE(Operand),
    /// je {0}
    JE(Operand),
    /// jne {0}
    JNE(Operand),
    /// nop
    NOP,
    /// int {0}
    INT(Operand),

    /// out {0} {1}
    OUT(Operand, Operand),

    /// dead instruction
    DEAD,

    /// call {function}
    CALL {
        function: Operand,
        nr_input_args: usize,
        nr_output_args: usize,
    },

    /// label
    LABEL,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum MemoryScale {
    /// * 1
    S1,
    /// * 2
    S2,
    /// * 4
    S4,
    /// * 8
    S8,
}

impl Into<i32> for MemoryScale {
    fn into(self) -> i32 {
        match self {
            MemoryScale::S1 => 1,
            MemoryScale::S2 => 2,
            MemoryScale::S4 => 4,
            MemoryScale::S8 => 8,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OperandKind {
    Immediate(u64),
    Memory {
        base: Option<Register>,
        index: Option<Register>,
        scale: MemoryScale,
        displacement: i32,
        segment_override: Option<SegmentRegister>,
    },
    Register(Register),
    Target(Ref<X86Block>),
}

impl Display for Operand {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}:{}", self.kind, self.width_in_bits)
    }
}

impl Display for OperandKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            OperandKind::Immediate(immval) => write!(f, "${immval}"),
            OperandKind::Memory {
                base,
                index,
                scale,
                displacement,
                segment_override,
            } => {
                if let Some(segment_override) = segment_override {
                    write!(f, "{segment_override}")?;
                }

                write!(f, "{displacement}(")?;

                if let Some(base) = base {
                    write!(f, "{base}")?;
                } else {
                    write!(f, "%riz")?;
                }

                if let Some(index) = index {
                    write!(f, ", {index}, {scale}")?;
                }

                write!(f, ")")
            }
            OperandKind::Register(reg) => write!(f, "{reg}"),
            OperandKind::Target(tgt) => write!(f, "{tgt:?}"),
        }
    }
}

impl Debug for OperandKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Immediate(arg0) => f.debug_tuple("Immediate").field(arg0).finish(),
            Self::Memory {
                base,
                index,
                scale,
                displacement,
                segment_override,
            } => f
                .debug_struct("Memory")
                .field("base", base)
                .field("index", index)
                .field("scale", scale)
                .field("displacement", displacement)
                .field("segment_override", segment_override)
                .finish(),
            Self::Register(arg0) => f.debug_tuple("Register").field(arg0).finish(),
            Self::Target(arg0) => write!(f, "{arg0:?}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]

pub struct Operand {
    pub kind: OperandKind,
    pub width_in_bits: Width,
}

impl Operand {
    pub fn kind(&self) -> &OperandKind {
        &self.kind
    }

    pub fn width(&self) -> Width {
        self.width_in_bits
    }
    pub fn set_width(&mut self, width: Width) {
        self.width_in_bits = width;
    }

    pub fn imm(width_in_bits: Width, value: u64) -> Operand {
        Operand {
            kind: OperandKind::Immediate(value),
            width_in_bits,
        }
    }

    pub fn preg(width_in_bits: Width, reg: PhysicalRegister) -> Operand {
        if reg.is_gpr() && width_in_bits == Width::_128 {
            panic!();
        }

        Operand {
            kind: OperandKind::Register(Register::Physical(reg)),
            width_in_bits,
        }
    }

    pub fn vreg(width_in_bits: Width, reg: usize) -> Operand {
        Operand {
            kind: OperandKind::Register(Register::Virtual(reg)),
            width_in_bits,
        }
    }

    pub fn greg(width_in_bits: Width, reg: usize) -> Operand {
        Operand {
            kind: OperandKind::Register(Register::Global(reg)),
            width_in_bits,
        }
    }

    pub fn mem_base(width_in_bits: Width, base: Register) -> Operand {
        Self::mem_base_displ(width_in_bits, base, 0)
    }

    pub fn mem_base_displ(width_in_bits: Width, base: Register, displacement: i32) -> Operand {
        Operand {
            kind: OperandKind::Memory {
                base: Some(base),
                index: None,
                scale: MemoryScale::S1,
                displacement,
                segment_override: None,
            },
            width_in_bits,
        }
    }

    pub fn mem_base_idx_scale(
        width_in_bits: Width,
        base: Register,
        idx: Register,
        scale: MemoryScale,
    ) -> Operand {
        Self::mem_base_idx_scale_displ(width_in_bits, base, idx, scale, 0)
    }

    pub fn mem_base_idx_scale_displ(
        width_in_bits: Width,
        base: Register,
        idx: Register,
        scale: MemoryScale,
        displacement: i32,
    ) -> Operand {
        Operand {
            kind: OperandKind::Memory {
                base: Some(base),
                index: Some(idx),
                scale,
                displacement,
                segment_override: None,
            },
            width_in_bits,
        }
    }

    pub fn mem_seg_displ(
        width_in_bits: u32,
        segment: SegmentRegister,
        displacement: i32,
    ) -> Operand {
        Operand {
            kind: OperandKind::Memory {
                base: None,
                index: None,
                scale: MemoryScale::S1,
                displacement,
                segment_override: Some(segment),
            },
            width_in_bits: Width::from_uncanonicalized(width_in_bits).unwrap(),
        }
    }

    pub fn target(target: Ref<X86Block>) -> Self {
        Self {
            kind: OperandKind::Target(target),
            width_in_bits: Width::_64, // todo: not really true, fix this
        }
    }

    pub fn as_register(&self) -> Option<Register> {
        match self.kind {
            OperandKind::Register(r) => Some(r),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction(pub Opcode);

macro_rules! alu_op {
    ($gen_name: ident, $opcode: ident) => {
        pub fn $gen_name(src: Operand, dst: Operand) -> Self {
            // todo: re-enable me
            // if src.width() != dst.width() {
            //     panic!("different widths: {src} {dst}")
            // }
            Instruction(Opcode::$opcode(src, dst))
        }
    };
}

pub enum OperandDirection {
    None,
    In,
    Out,
    InOut,
}

/// UseDef
#[derive(Debug, displaydoc::Display)]
pub enum UseDef {
    /// use {0}
    Use(Register),
    /// def {0}
    Def(Register),
    /// usedef {0}
    UseDef(Register),
}

impl UseDef {
    pub fn from_operand_direction(direction: OperandDirection, register: Register) -> Option<Self> {
        match direction {
            OperandDirection::None => None,
            OperandDirection::In => Some(Self::Use(register)),
            OperandDirection::Out => Some(Self::Def(register)),
            OperandDirection::InOut => Some(Self::UseDef(register)),
        }
    }

    pub fn has_use(&self) -> bool {
        matches!(self, Self::Use(_) | Self::UseDef(_))
    }

    pub fn has_def(&self) -> bool {
        matches!(self, Self::Def(_) | Self::UseDef(_))
    }

    pub fn is_usedef(&self) -> bool {
        matches!(self, Self::UseDef(_))
    }
}

/// UseDef
#[derive(Debug, displaydoc::Display)]
pub enum UseDefMut<'a> {
    /// use {0}
    Use(&'a mut Register),
    /// def {0}
    Def(&'a mut Register),
    /// usedef {0}
    UseDef(&'a mut Register),
}

impl<'a> UseDefMut<'a> {
    pub fn from_operand_direction(
        direction: OperandDirection,
        register: &'a mut Register,
    ) -> Option<Self> {
        match direction {
            OperandDirection::None => None,
            OperandDirection::In => Some(Self::Use(register)),
            OperandDirection::Out => Some(Self::Def(register)),
            OperandDirection::InOut => Some(Self::UseDef(register)),
        }
    }

    pub fn has_use(&self) -> bool {
        matches!(self, Self::Use(_) | Self::UseDef(_))
    }

    pub fn has_def(&self) -> bool {
        matches!(self, Self::Def(_) | Self::UseDef(_))
    }

    pub fn is_usedef(&self) -> bool {
        matches!(self, Self::UseDef(_))
    }
}

impl Display for Instruction {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn memory_operand_to_iced(
    base: PhysicalRegister,
    index: Option<Register>,
    scale: MemoryScale,
    displacement: i32,
) -> AsmMemoryOperand {
    let mut mem = AsmRegister64::from(base) + displacement;

    if let Some(Register::Physical(index)) = index {
        let scale: i32 = match scale {
            MemoryScale::S1 => 1,
            MemoryScale::S2 => 2,
            MemoryScale::S4 => 4,
            MemoryScale::S8 => 8,
        }
        .into();

        mem = mem + AsmRegister64::from(index) * scale;
    }

    mem
}

fn segment_memory_operand_to_iced(
    segment: SegmentRegister,
    index: Option<Register>,
    scale: MemoryScale,
    displacement: i32,
) -> AsmMemoryOperand {
    let mut mem = AsmMemoryOperand::from(displacement);

    if let Some(Register::Physical(index)) = index {
        let scale: i32 = match scale {
            MemoryScale::S1 => 1,
            MemoryScale::S2 => 2,
            MemoryScale::S4 => 4,
            MemoryScale::S8 => 8,
        }
        .into();

        mem = mem + AsmRegister64::from(index) * scale;
    }

    match segment {
        SegmentRegister::FS => mem.fs(),
        SegmentRegister::GS => mem.gs(),
    }
}

impl Instruction {
    pub fn adc(a: Operand, b: Operand, c: Operand) -> Self {
        Self(Opcode::ADC(a, b, c))
    }

    pub fn mov(src: Operand, dst: Operand) -> Result<Self, Error> {
        // todo: remove these checks or enforce them earlier
        if src.width() == Width::_128
            && let OperandKind::Register(Register::Physical(phys)) = src.kind()
            && phys.is_gpr()
        {
            return Err(Error::OversizeGeneralRegister(src));
        }

        if dst.width() == Width::_128
            && let OperandKind::Register(Register::Physical(phys)) = dst.kind()
            && phys.is_gpr()
        {
            return Err(Error::OversizeGeneralRegister(dst));
        }

        if src.width() != Width::_128 && src.width() != dst.width() {
            return Err(Error::MovWidthMismatch { src, dst });
        }

        if let OperandKind::Immediate(_) = src.kind()
            && src.width() == Width::_128
        {
            return Err(Error::MovImmediateSSE { src, dst });
        }

        Ok(Self(Opcode::MOV(src, dst)))
    }

    pub fn movzx(src: Operand, dst: Operand) -> Result<Self, Error> {
        if src.width() >= dst.width() {
            return Err(Error::MovZeroExtendDestinationNotGreater { src, dst });
        }

        if let OperandKind::Immediate(_) = src.kind()
            && dst.width() == Width::_128
        {
            return Err(Error::MovImmediateSSE { src, dst });
        }

        Ok(Self(Opcode::MOVZX(src, dst)))
    }

    pub fn movsx(src: Operand, dst: Operand) -> Self {
        assert!(
            src.width() < dst.width(),
            "can't sign extend {src} to {dst} (dst width must be greater than src width)"
        );
        Self(Opcode::MOVSX(src, dst))
    }

    pub fn lea(src: Operand, dst: Operand) -> Self {
        Self(Opcode::LEA(src, dst))
    }

    pub fn and(src: Operand, dst: Operand) -> Self {
        Self(Opcode::AND(src, dst))
    }

    pub fn mul(src: Operand, dst_lo: Operand, dst_hi: Operand) -> Self {
        Self(Opcode::MUL(src, dst_lo, dst_hi))
    }

    pub fn imul(src: Operand, dst: Operand) -> Self {
        Self(Opcode::IMUL(src, dst))
    }

    pub fn imul1(src: Operand, dst_lo: Operand, dst_hi: Operand) -> Self {
        Self(Opcode::IMUL1(src, dst_lo, dst_hi))
    }

    pub fn idiv(divisor: Operand) -> Self {
        Self(Opcode::IDIV(divisor))
    }
    pub fn div(divisor: Operand) -> Self {
        Self(Opcode::DIV(divisor))
    }
    pub fn cqo() -> Self {
        Self(Opcode::CQO)
    }

    pub fn shl(amount: Operand, op0: Operand) -> Self {
        Self(Opcode::SHL(amount, op0))
    }

    pub fn shr(amount: Operand, op0: Operand) -> Self {
        if op0.width() > Width::_64
            && let OperandKind::Immediate(amount) = amount.kind()
            && amount % 8 != 0
        {
            panic!("{amount:?} {op0:?}")
        }
        Self(Opcode::SHR(amount, op0))
    }

    pub fn shld(amount: Operand, hi: Operand, lo: Operand) -> Self {
        Self(Opcode::SHLD(amount, hi, lo))
    }

    pub fn shrd(amount: Operand, hi: Operand, lo: Operand) -> Self {
        Self(Opcode::SHRD(amount, hi, lo))
    }

    pub fn sar(amount: Operand, op0: Operand) -> Self {
        Self(Opcode::SAR(amount, op0))
    }

    pub fn bextr(ctrl: Operand, src: Operand, dst: Operand) -> Self {
        Self(Opcode::BEXTR(ctrl, src, dst))
    }

    pub fn punpckl(src: Operand, dst: Operand) -> Self {
        Self(Opcode::PUNPCKL(src, dst))
    }

    pub fn pinsr(index: Operand, src: Operand, dst: Operand) -> Self {
        Self(Opcode::PINSR(index, src, dst))
    }

    pub fn pextr(index: Operand, src: Operand, dst: Operand) -> Self {
        Self(Opcode::PEXTR(index, src, dst))
    }

    pub fn jmp(block: Ref<X86Block>) -> Self {
        Self(Opcode::JMP(Operand::target(block)))
    }

    pub fn push(src: Operand) -> Self {
        Self(Opcode::PUSH(src))
    }

    pub fn pop(dest: Operand) -> Self {
        Self(Opcode::POP(dest))
    }

    pub fn ret() -> Self {
        Self(Opcode::RET)
    }

    pub fn nop() -> Self {
        Self(Opcode::NOP)
    }

    pub fn test(op0: Operand, op1: Operand) -> Result<Self, Error> {
        if let (OperandKind::Immediate(_), OperandKind::Immediate(_)) = (op0.kind(), op1.kind()) {
            return Err(Error::TestImmediates);
        }
        Ok(Self(Opcode::TEST(op0, op1)))
    }

    pub fn cmp(op0: Operand, op1: Operand) -> Self {
        Self(Opcode::CMP(op0, op1))
    }
    pub fn seto(r: Operand) -> Self {
        Self(Opcode::SETO(r))
    }
    pub fn setc(r: Operand) -> Self {
        Self(Opcode::SETC(r))
    }
    pub fn sete(r: Operand) -> Self {
        Self(Opcode::SETE(r))
    }

    pub fn sets(r: Operand) -> Self {
        Self(Opcode::SETS(r))
    }

    pub fn setne(r: Operand) -> Self {
        Self(Opcode::SETNE(r))
    }
    pub fn setnz(r: Operand) -> Self {
        Self(Opcode::SETNZ(r))
    }

    pub fn setb(r: Operand) -> Self {
        Self(Opcode::SETB(r))
    }
    pub fn setl(r: Operand) -> Self {
        Self(Opcode::SETL(r))
    }

    pub fn setle(r: Operand) -> Self {
        Self(Opcode::SETLE(r))
    }
    pub fn setge(r: Operand) -> Self {
        Self(Opcode::SETGE(r))
    }
    pub fn setg(r: Operand) -> Self {
        Self(Opcode::SETG(r))
    }
    pub fn setbe(r: Operand) -> Self {
        Self(Opcode::SETBE(r))
    }

    pub fn seta(r: Operand) -> Self {
        Self(Opcode::SETA(r))
    }
    pub fn setae(r: Operand) -> Self {
        Self(Opcode::SETAE(r))
    }

    pub fn je(block: Ref<X86Block>) -> Self {
        Self(Opcode::JE(Operand::target(block)))
    }

    pub fn jne(block: Ref<X86Block>) -> Self {
        Self(Opcode::JNE(Operand::target(block)))
    }

    pub fn out(port: Operand, value: Operand) -> Self {
        Self(Opcode::OUT(port, value))
    }

    pub fn not(r: Operand) -> Self {
        Self(Opcode::NOT(r))
    }

    pub fn neg(r: Operand) -> Self {
        Self(Opcode::NEG(r))
    }

    pub fn int(n: Operand) -> Self {
        Self(Opcode::INT(n))
    }

    pub fn cmove(src: Operand, dest: Operand) -> Self {
        Self(Opcode::CMOVE(src, dest))
    }

    pub fn cmovne(src: Operand, dest: Operand) -> Self {
        Self(Opcode::CMOVNE(src, dest))
    }

    pub fn call(function: Operand, nr_input_args: usize, nr_output_args: usize) -> Self {
        Self(Opcode::CALL {
            function,
            nr_input_args,
            nr_output_args,
        })
    }

    alu_op!(add, ADD);
    alu_op!(sub, SUB);
    alu_op!(or, OR);
    alu_op!(xor, XOR);

    pub fn encode(
        &self,
        assembler: &mut CodeAssembler,
        label_map: &HashMapA<Ref<X86Block>, CodeLabel, BumpAllocatorRef>,
    ) {
        use {
            Opcode::*,
            OperandKind::{Immediate as I, Memory as M, Register as R, Target as T},
            Register::Physical as PHYS,
        };

        match &self.0 {
            // do not emit dead instructions
            DEAD | LABEL => (),
            NOP => assembler.nop().unwrap(),
            MOV(src, dst) => mov::encode(assembler, src, dst),
            MOVZX(src, dst) => movzx::encode(assembler, src, dst),
            MOVSX(src, dst) => movsx::encode(assembler, src, dst),
            SHL(amount, value) => shl::encode(assembler, amount, value),
            SHR(amount, value) => shr::encode(assembler, amount, value),
            AND(src, dst) => and::encode(assembler, src, dst),
            SETNE(dst) => setne::encode(assembler, dst),
            LEA(src, dst) => lea::encode(assembler, src, dst),
            ADD(src, dst) => add::encode(assembler, src, dst),
            SUB(src, dst) => sub::encode(assembler, src, dst),
            TEST(src, dst) => test::encode(assembler, src, dst),
            OR(src, dst) => or::encode(assembler, src, dst),
            ADC(src, dst, carry) => adc::encode(assembler, src, dst, carry),
            CMP(left, right) => cmp::encode(assembler, left, right),
            XOR(src, dst) => xor::encode(assembler, src, dst),
            NOT(dst) => not::encode(assembler, dst),

            // control flow
            JNE(tgt) => jne::encode(assembler, label_map, tgt),
            JE(Operand {
                kind: T(target), ..
            }) => {
                let label = label_map
                    .get(target)
                    .unwrap_or_else(|| panic!("no label for {target:?} found"))
                    .clone();
                assembler.je(label).unwrap();
            }
            JMP(Operand {
                kind: T(target), ..
            }) => {
                let label = label_map
                    .get(target)
                    .unwrap_or_else(|| panic!("no label for {target:?} found"))
                    .clone();
                assembler.jmp(label.clone()).unwrap();
            }
            JMP(Operand {
                kind:
                    M {
                        base: Some(PHYS(base)),
                        index,
                        scale,
                        displacement,
                        ..
                    },
                ..
            }) => {
                assembler
                    .jmp(qword_ptr(memory_operand_to_iced(
                        *base,
                        *index,
                        *scale,
                        *displacement,
                    )))
                    .unwrap();
            }
            RET => {
                assembler.ret().unwrap();
            }

            SETA(Operand {
                kind: R(PHYS(dst)), ..
            }) => {
                assembler.seta::<AsmRegister8>(dst.into()).unwrap();
            }
            SETG(Operand {
                kind: R(PHYS(dst)), ..
            }) => {
                assembler.setg::<AsmRegister8>(dst.into()).unwrap();
            }
            SETAE(Operand {
                kind: R(PHYS(dst)), ..
            }) => {
                assembler.setae::<AsmRegister8>(dst.into()).unwrap();
            }
            SETE(Operand {
                kind: R(PHYS(dst)), ..
            }) => {
                assembler.sete::<AsmRegister8>(dst.into()).unwrap();
            }
            SETE(Operand {
                kind:
                    M {
                        base: Some(PHYS(base)),
                        index,
                        scale,
                        displacement,
                        ..
                    },
                width_in_bits: Width::_8,
            }) => {
                assembler
                    .sete(memory_operand_to_iced(*base, *index, *scale, *displacement))
                    .unwrap();
            }
            SETO(Operand {
                kind: R(PHYS(dst)), ..
            }) => {
                assembler.seto::<AsmRegister8>(dst.into()).unwrap();
            }
            SETO(Operand {
                kind:
                    M {
                        base: Some(PHYS(base)),
                        index,
                        scale,
                        displacement,
                        ..
                    },
                width_in_bits: Width::_8,
            }) => {
                assembler
                    .seto(memory_operand_to_iced(*base, *index, *scale, *displacement))
                    .unwrap();
            }
            SETC(Operand {
                kind: R(PHYS(dst)), ..
            }) => {
                assembler.setc::<AsmRegister8>(dst.into()).unwrap();
            }
            SETC(Operand {
                kind:
                    M {
                        base: Some(PHYS(base)),
                        index,
                        scale,
                        displacement,
                        ..
                    },
                width_in_bits: Width::_8,
            }) => {
                assembler
                    .setc(memory_operand_to_iced(*base, *index, *scale, *displacement))
                    .unwrap();
            }
            SETS(Operand {
                kind: R(PHYS(dst)), ..
            }) => {
                assembler.sets::<AsmRegister8>(dst.into()).unwrap();
            }
            SETS(Operand {
                kind:
                    M {
                        base: Some(PHYS(base)),
                        index,
                        scale,
                        displacement,
                        ..
                    },
                width_in_bits: Width::_8,
            }) => {
                assembler
                    .sets(memory_operand_to_iced(*base, *index, *scale, *displacement))
                    .unwrap();
            }
            SETGE(Operand {
                kind: R(PHYS(dst)), ..
            }) => {
                assembler.setge::<AsmRegister8>(dst.into()).unwrap();
            }

            NEG(Operand {
                kind: R(PHYS(value)),
                ..
            }) => assembler.neg::<AsmRegister64>(value.into()).unwrap(),
            SAR(
                Operand {
                    kind: R(PHYS(amount)),
                    width_in_bits: Width::_8,
                },
                Operand {
                    kind: R(PHYS(value)),
                    width_in_bits: Width::_64,
                },
            ) => {
                assembler
                    .sar::<AsmRegister64, AsmRegister8>(value.into(), amount.into())
                    .unwrap();
            }
            SAR(
                Operand {
                    kind: R(PHYS(amount)),
                    width_in_bits: Width::_8,
                },
                Operand {
                    kind: R(PHYS(value)),
                    width_in_bits: Width::_32,
                },
            ) => {
                assembler
                    .sar::<AsmRegister32, AsmRegister8>(value.into(), amount.into())
                    .unwrap();
            }
            SAR(
                Operand {
                    kind: I(amount),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(value)),
                    width_in_bits: Width::_64,
                },
            ) => {
                assembler
                    .sar::<AsmRegister64, i32>(value.into(), i32::try_from(*amount).unwrap())
                    .unwrap();
            }
            SAR(
                Operand {
                    kind: I(amount),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(value)),
                    width_in_bits: Width::_32,
                },
            ) => {
                assembler
                    .sar::<AsmRegister32, i32>(value.into(), i32::try_from(*amount).unwrap())
                    .unwrap();
            }
            BEXTR(
                Operand {
                    kind: R(PHYS(ctrl)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_64,
                },
            ) => {
                assembler
                    .bextr::<AsmRegister64, AsmRegister64, AsmRegister64>(
                        dst.into(),
                        src.into(),
                        ctrl.into(),
                    )
                    .unwrap();
            }
            INT(Operand { kind: I(n), .. }) => {
                assembler.int(i32::try_from(*n).unwrap()).unwrap();
            }
            PUSH(Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_64,
            }) => {
                assembler.push::<AsmRegister64>(src.into()).unwrap();
            }
            POP(Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            }) => {
                assembler.pop::<AsmRegister64>(dst.into()).unwrap();
            }
            SETB(Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_8,
            }) => {
                assembler.setne::<AsmRegister8>(dst.into()).unwrap();
            }
            SETNZ(Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_8,
            }) => assembler.setnz::<AsmRegister8>(dst.into()).unwrap(),
            SETBE(Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_8,
            }) => {
                assembler.setbe::<AsmRegister8>(dst.into()).unwrap();
            }
            SETLE(Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_8,
            }) => {
                assembler.setle::<AsmRegister8>(dst.into()).unwrap();
            }
            SETL(Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_8,
            }) => {
                assembler.setl::<AsmRegister8>(dst.into()).unwrap();
            }

            CMOVE(
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_64,
                },
            ) => {
                assembler
                    .cmove::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
                    .unwrap();
            }
            CMOVNE(
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_64,
                },
            ) => {
                assembler
                    .cmovne::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
                    .unwrap();
            }
            CMOVNE(
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_32,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_32,
                },
            ) => {
                assembler
                    .cmovne::<AsmRegister32, AsmRegister32>(dst.into(), src.into())
                    .unwrap();
            }
            IMUL(
                Operand {
                    kind: I(left),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_64,
                },
            ) => assembler
                .imul_3::<AsmRegister64, AsmRegister64, i32>(
                    dst.into(),
                    dst.into(),
                    i32::try_from(*left).unwrap(),
                )
                .unwrap(),
            IMUL(
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_64,
                },
            ) => assembler
                .imul_2::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
                .unwrap(),

            MUL(
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(dst_lo)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(dst_hi)),
                    width_in_bits: Width::_64,
                },
            ) => {
                assert_eq!(*dst_hi, PhysicalRegister::RDX);
                assert_eq!(*dst_lo, PhysicalRegister::RAX);
                assembler.mul::<AsmRegister64>(src.into()).unwrap()
            }
            IMUL1(
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(dst_lo)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(dst_hi)),
                    width_in_bits: Width::_64,
                },
            ) => {
                assert_eq!(*dst_hi, PhysicalRegister::RDX);
                assert_eq!(*dst_lo, PhysicalRegister::RAX);
                assembler.imul::<AsmRegister64>(src.into()).unwrap()
            }
            CQO => {
                assembler.cqo().unwrap();
            }
            IDIV(Operand {
                kind: R(PHYS(div)),
                width_in_bits: Width::_64,
            }) => {
                assembler.idiv::<AsmRegister64>(div.into()).unwrap();
            }
            DIV(Operand {
                kind: R(PHYS(div)),
                width_in_bits: Width::_64,
            }) => {
                assembler.div::<AsmRegister64>(div.into()).unwrap();
            }

            CALL {
                function:
                    Operand {
                        kind: R(PHYS(tgt)),
                        width_in_bits: Width::_64,
                    },
                ..
            } => {
                assembler.call::<AsmRegister64>(tgt.into()).unwrap();
            }

            OUT(
                Operand {
                    kind: I(port),
                    width_in_bits: Width::_8,
                },
                Operand {
                    kind: R(PHYS(value)),
                    width_in_bits: Width::_8,
                },
            ) => assembler
                .out::<i32, AsmRegister8>((*port).try_into().unwrap(), value.into())
                .unwrap(),

            PINSR(
                Operand {
                    kind: I(index),
                    width_in_bits: Width::_8,
                },
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_16,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_128,
                },
            ) => {
                // does the low word from an r32 so safe, todo check this
                assembler
                    .pinsrw::<AsmRegisterXmm, AsmRegister32, i32>(
                        dst.try_into().unwrap(),
                        src.into(),
                        i32::try_from(*index).unwrap(),
                    )
                    .unwrap();
            }

            PINSR(
                Operand {
                    kind: I(index),
                    width_in_bits: Width::_8,
                },
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_32,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_128,
                },
            ) => {
                assembler
                    .pinsrd::<AsmRegisterXmm, AsmRegister32, i32>(
                        dst.try_into().unwrap(),
                        src.into(),
                        i32::try_from(*index).unwrap(),
                    )
                    .unwrap();
            }

            PINSR(
                Operand {
                    kind: I(index),
                    width_in_bits: Width::_8,
                },
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_128,
                },
            ) => {
                assembler
                    .pinsrq::<AsmRegisterXmm, AsmRegister64, i32>(
                        dst.try_into().unwrap(),
                        src.into(),
                        i32::try_from(*index).unwrap(),
                    )
                    .unwrap();
            }

            PINSR(
                Operand {
                    kind: I(index),
                    width_in_bits: Width::_8,
                },
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_8,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_128,
                },
            ) => {
                assembler
                    .pinsrb::<AsmRegisterXmm, AsmRegister32, i32>(
                        dst.try_into().unwrap(),
                        src.into(),
                        i32::try_from(*index).unwrap(),
                    )
                    .unwrap();
            }

            PUNPCKL(
                Operand {
                    kind: R(PHYS(src)),
                    width_in_bits: Width::_128,
                },
                Operand {
                    kind: R(PHYS(dst)),
                    width_in_bits: Width::_128,
                },
            ) => assembler
                .punpcklqdq::<AsmRegisterXmm, AsmRegisterXmm>(
                    dst.try_into().unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap(),

            _ => panic!("cannot encode this instruction {}", self),
        }
    }

    pub fn get_operands_mut(
        &mut self,
    ) -> impl Iterator<Item = Option<(OperandDirection, &mut Operand)>> + '_ {
        match &mut self.0 {
            Opcode::MOV(src, dst)
            | Opcode::MOVZX(src, dst)
            | Opcode::MOVSX(src, dst)
            | Opcode::LEA(src, dst)
            | Opcode::CMOVE(src, dst)
            | Opcode::CMOVNE(src, dst) => [
                Some((OperandDirection::In, src)),
                Some((OperandDirection::Out, dst)),
                None,
            ]
            .into_iter(),
            Opcode::SHL(src, dst)
            | Opcode::SHR(src, dst)
            | Opcode::SAR(src, dst)
            | Opcode::OR(src, dst)
            | Opcode::XOR(src, dst)
            | Opcode::ADD(src, dst)
            | Opcode::SUB(src, dst)
            | Opcode::AND(src, dst)
            | Opcode::IMUL(src, dst)
            | Opcode::PUNPCKL(src, dst) => [
                Some((OperandDirection::In, src)),
                Some((OperandDirection::InOut, dst)),
                None,
            ]
            .into_iter(),
            Opcode::MUL(src, dst_lo, dst_hi) => [
                Some((OperandDirection::In, src)),
                Some((OperandDirection::InOut, dst_lo)),
                Some((OperandDirection::Out, dst_hi)),
            ]
            .into_iter(),
            Opcode::IMUL1(src, dst_lo, dst_hi) => [
                Some((OperandDirection::In, src)),
                Some((OperandDirection::InOut, dst_lo)),
                Some((OperandDirection::Out, dst_hi)),
            ]
            .into_iter(),
            Opcode::SHRD(amount, hi, lo) | Opcode::SHLD(amount, hi, lo) => [
                Some((OperandDirection::In, amount)),
                Some((OperandDirection::In, lo)),
                Some((OperandDirection::InOut, hi)),
            ]
            .into_iter(),
            Opcode::IDIV(divisor) => {
                [Some((OperandDirection::In, divisor)), None, None].into_iter()
            }
            Opcode::DIV(divisor) => [Some((OperandDirection::In, divisor)), None, None].into_iter(),
            Opcode::JMP(tgt) | Opcode::JNE(tgt) | Opcode::JE(tgt) => {
                [Some((OperandDirection::In, tgt)), None, None].into_iter()
            }
            Opcode::CALL { function, .. } => {
                [Some((OperandDirection::In, function)), None, None].into_iter()
            }
            Opcode::RET | Opcode::NOP => [None, None, None].into_iter(),
            Opcode::TEST(op0, op1) | Opcode::CMP(op0, op1) => [
                Some((OperandDirection::In, op0)),
                Some((OperandDirection::In, op1)),
                None,
            ]
            .into_iter(),
            Opcode::SETE(r)
            | Opcode::SETNE(r)
            | Opcode::SETNZ(r)
            | Opcode::SETB(r)
            | Opcode::SETBE(r)
            | Opcode::SETA(r)
            | Opcode::SETG(r)
            | Opcode::SETAE(r)
            | Opcode::SETS(r)
            | Opcode::SETO(r)
            | Opcode::SETC(r)
            | Opcode::SETGE(r)
            | Opcode::SETL(r)
            | Opcode::SETLE(r) => [Some((OperandDirection::Out, r)), None, None].into_iter(),
            Opcode::NOT(r) | Opcode::NEG(r) => {
                [Some((OperandDirection::InOut, r)), None, None].into_iter()
            }
            Opcode::BEXTR(ctrl, src, dst) => [
                Some((OperandDirection::In, ctrl)),
                Some((OperandDirection::In, src)),
                Some((OperandDirection::Out, dst)),
            ]
            .into_iter(),
            Opcode::INT(n) => [Some((OperandDirection::In, n)), None, None].into_iter(),
            Opcode::ADC(a, b, c) => [
                Some((OperandDirection::In, a)),
                Some((OperandDirection::In, b)),
                Some((OperandDirection::InOut, c)),
            ]
            .into_iter(),
            Opcode::PINSR(index, src, dst) => [
                Some((OperandDirection::In, index)),
                Some((OperandDirection::In, src)),
                Some((OperandDirection::InOut, dst)),
            ]
            .into_iter(),
            Opcode::PEXTR(index, src, dst) => [
                Some((OperandDirection::In, index)),
                Some((OperandDirection::In, src)),
                Some((OperandDirection::Out, dst)),
            ]
            .into_iter(),
            Opcode::PUSH(src) => [Some((OperandDirection::In, src)), None, None].into_iter(),
            Opcode::POP(dest) => [Some((OperandDirection::Out, dest)), None, None].into_iter(),
            Opcode::DEAD => panic!(),
            Opcode::OUT(port, value) => [
                Some((OperandDirection::In, port)),
                Some((OperandDirection::In, value)),
                None,
            ]
            .into_iter(),
            Opcode::CQO => [None, None, None].into_iter(),
            Opcode::LABEL => [None, None, None].into_iter(),
        }
    }

    pub fn get_apparent_operands(&self) -> Vec<(OperandDirection, Operand)> {
        match self.0 {
            Opcode::MOV(src, dst)
            | Opcode::MOVZX(src, dst)
            | Opcode::MOVSX(src, dst)
            | Opcode::LEA(src, dst)
            | Opcode::CMOVE(src, dst)
            | Opcode::CMOVNE(src, dst) => {
                [(OperandDirection::In, src), (OperandDirection::Out, dst)]
                    .into_iter()
                    .collect()
            }
            Opcode::SHL(src, dst)
            | Opcode::SHR(src, dst)
            | Opcode::SAR(src, dst)
            | Opcode::OR(src, dst)
            | Opcode::XOR(src, dst)
            | Opcode::ADD(src, dst)
            | Opcode::SUB(src, dst)
            | Opcode::AND(src, dst)
            | Opcode::IMUL(src, dst)
            | Opcode::PUNPCKL(src, dst) => {
                [(OperandDirection::In, src), (OperandDirection::InOut, dst)]
                    .into_iter()
                    .collect()
            }
            Opcode::MUL(src, dst_lo, dst_hi) => [
                (OperandDirection::In, src),
                (OperandDirection::InOut, dst_lo),
                (OperandDirection::Out, dst_hi),
            ]
            .into_iter()
            .collect(),
            Opcode::IMUL1(src, dst_lo, dst_hi) => [
                (OperandDirection::In, src),
                (OperandDirection::InOut, dst_lo),
                (OperandDirection::Out, dst_hi),
            ]
            .into_iter()
            .collect(),
            Opcode::SHRD(amount, hi, lo) | Opcode::SHLD(amount, hi, lo) => [
                (OperandDirection::In, amount),
                (OperandDirection::In, lo),
                (OperandDirection::InOut, hi),
            ]
            .into_iter()
            .collect(),
            Opcode::IDIV(divisor) => {
                let width = divisor.width();
                [
                    (
                        OperandDirection::InOut,
                        Operand::preg(width, PhysicalRegister::RDX),
                    ),
                    (
                        OperandDirection::InOut,
                        Operand::preg(width, PhysicalRegister::RAX),
                    ),
                    (OperandDirection::In, divisor),
                ]
                .into_iter()
                .collect()
            }
            Opcode::DIV(divisor) => {
                let width = divisor.width();
                [
                    (
                        OperandDirection::InOut,
                        Operand::preg(width, PhysicalRegister::RDX),
                    ),
                    (
                        OperandDirection::InOut,
                        Operand::preg(width, PhysicalRegister::RAX),
                    ),
                    (OperandDirection::In, divisor),
                ]
                .into_iter()
                .collect()
            }
            Opcode::JMP(tgt) | Opcode::JNE(tgt) | Opcode::JE(tgt) => {
                [((OperandDirection::In, tgt))].into_iter().collect()
            }
            Opcode::RET | Opcode::NOP => alloc::vec![],
            Opcode::TEST(op0, op1) | Opcode::CMP(op0, op1) => {
                [((OperandDirection::In, op0)), ((OperandDirection::In, op1))]
                    .into_iter()
                    .collect()
            }
            Opcode::SETE(r)
            | Opcode::SETNE(r)
            | Opcode::SETNZ(r)
            | Opcode::SETB(r)
            | Opcode::SETBE(r)
            | Opcode::SETA(r)
            | Opcode::SETG(r)
            | Opcode::SETAE(r)
            | Opcode::SETS(r)
            | Opcode::SETO(r)
            | Opcode::SETC(r)
            | Opcode::SETGE(r)
            | Opcode::SETL(r)
            | Opcode::SETLE(r) => [((OperandDirection::Out, r))].into_iter().collect(),
            Opcode::NOT(r) | Opcode::NEG(r) => {
                [((OperandDirection::InOut, r))].into_iter().collect()
            }
            Opcode::BEXTR(ctrl, src, dst) => [
                ((OperandDirection::In, ctrl)),
                ((OperandDirection::In, src)),
                ((OperandDirection::Out, dst)),
            ]
            .into_iter()
            .collect(),
            Opcode::PINSR(index, src, dst) => [
                ((OperandDirection::In, index)),
                ((OperandDirection::In, src)),
                ((OperandDirection::InOut, dst)),
            ]
            .into_iter()
            .collect(),
            Opcode::PEXTR(index, src, dst) => [
                ((OperandDirection::In, index)),
                ((OperandDirection::In, src)),
                ((OperandDirection::Out, dst)),
            ]
            .into_iter()
            .collect(),
            Opcode::INT(n) => [((OperandDirection::In, n))].into_iter().collect(),
            Opcode::ADC(a, b, c) => [
                ((OperandDirection::In, a)),
                ((OperandDirection::In, b)),
                ((OperandDirection::InOut, c)),
            ]
            .into_iter()
            .collect(),
            Opcode::PUSH(src) => [((OperandDirection::In, src))].into_iter().collect(),
            Opcode::POP(dest) => [((OperandDirection::Out, dest))].into_iter().collect(),
            Opcode::DEAD => [].into_iter().collect(),
            Opcode::OUT(port, value) => [
                ((OperandDirection::In, port)),
                ((OperandDirection::In, value)),
            ]
            .into_iter()
            .collect(),
            Opcode::CALL {
                function,
                nr_input_args,
                nr_output_args,
            } => [((OperandDirection::In, function))]
                .into_iter()
                .chain(
                    ARG_REGS
                        .iter()
                        .take(nr_input_args)
                        .map(|reg| (OperandDirection::In, Operand::preg(Width::_64, *reg))),
                )
                .chain(
                    [PhysicalRegister::RAX, PhysicalRegister::RDX]
                        .into_iter()
                        .take(nr_output_args)
                        .map(|reg| (OperandDirection::Out, Operand::preg(Width::_64, reg))),
                )
                .collect(),
            Opcode::LABEL => alloc::vec::Vec::default(),
            Opcode::CQO => [
                (
                    OperandDirection::InOut,
                    Operand::preg(Width::_64, PhysicalRegister::RAX),
                ),
                (
                    OperandDirection::Out,
                    Operand::preg(Width::_64, PhysicalRegister::RDX),
                ),
            ]
            .into_iter()
            .collect(),
        }
    }

    pub fn get_use_defs(&self) -> impl Iterator<Item = (UseDef, Width)> + '_ {
        self.get_apparent_operands()
            .into_iter()
            .filter_map(|(direction, operand)| match operand.kind() {
                OperandKind::Memory {
                    base: Some(base),
                    index: None,
                    ..
                } => Some(alloc::vec![(UseDef::Use(*base), Width::_64)]), //todo check me
                OperandKind::Memory {
                    base: Some(base),
                    index: Some(index),
                    ..
                } => Some(alloc::vec![
                    (UseDef::Use(*base), Width::_64),
                    (UseDef::Use(*index), Width::_64),
                ]),
                OperandKind::Register(register) => Some(alloc::vec![(
                    UseDef::from_operand_direction(direction, *register).unwrap(),
                    operand.width()
                )]),
                _ => None,
            })
            .flatten()
    }

    pub fn get_use_defs_mut(&'_ mut self) -> impl Iterator<Item = UseDefMut<'_>> + '_ {
        self.get_operands_mut()
            .flatten()
            .filter_map(|operand| match &mut operand.1.kind {
                OperandKind::Memory {
                    base: Some(base),
                    index: None,
                    ..
                } => Some(alloc::vec![UseDefMut::Use(base)]),
                OperandKind::Memory {
                    base: Some(base),
                    index: Some(index),
                    ..
                } => Some(alloc::vec![UseDefMut::Use(base), UseDefMut::Use(index)]),
                OperandKind::Register(register) => Some(alloc::vec![
                    UseDefMut::from_operand_direction(operand.0, register).unwrap(),
                ]),
                _ => None,
            })
            .flatten()
    }
}

/// Instruction encoding error
#[derive(Debug, Clone, displaydoc::Display, thiserror::Error)]
pub enum Error {
    /// Mov operands have different widths, src: {src}, dst: {dst}
    MovWidthMismatch { src: Operand, dst: Operand },
    /// Cannot move an immediate ({src}) into an SSE register ({dst})
    MovImmediateSSE { src: Operand, dst: Operand },
    /// Found general register greater than 64-bits wide: {0}
    OversizeGeneralRegister(Operand),
    /// Cannot zero extend {src} into equal-or-smaller destination {dst}
    MovZeroExtendDestinationNotGreater { src: Operand, dst: Operand },
    /// Cannot test two immedaites
    TestImmediates,
}
