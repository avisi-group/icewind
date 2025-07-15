use {
    crate::host::dbt::{
        Alloc,
        x86::{
            emitter::{ARG_REGS, X86Block},
            encoder::{
                instructions::{
                    adc, add, and, cmp, lea, mov, movsx, movzx, or, setne, shl, shr, sub, test, xor,
                },
                registers::{PhysicalRegister, Register, SegmentRegister},
                width::Width,
            },
        },
    },
    alloc::vec::Vec,
    common::{arena::Ref, hashmap::HashMapA},
    core::fmt::{Debug, Display, Formatter},
    derive_where::derive_where,
    displaydoc::Display,
    iced_x86::code_asm::{
        AsmMemoryOperand, AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, CodeAssembler,
        CodeLabel, qword_ptr,
    },
};

mod instructions;
pub mod registers;
pub mod width;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum Opcode<A: Alloc> {
    /// mov {0}, {1}
    MOV(Operand<A>, Operand<A>),
    /// movzx {0}, {1}
    MOVZX(Operand<A>, Operand<A>),
    /// movsx {0}, {1}
    MOVSX(Operand<A>, Operand<A>),
    /// cmove {0}, {1}
    CMOVE(Operand<A>, Operand<A>),
    /// cmovne {0}, {1}
    CMOVNE(Operand<A>, Operand<A>),

    /// lea {0}, {1}
    LEA(Operand<A>, Operand<A>),
    /// shl {0}, {1}
    SHL(Operand<A>, Operand<A>),
    /// shr {0}, {1}
    SHR(Operand<A>, Operand<A>),
    /// sar {0}, {1}
    SAR(Operand<A>, Operand<A>),
    /// add {0}, {1}
    ADD(Operand<A>, Operand<A>),
    /// adc {0}, {1}, {2}
    ADC(Operand<A>, Operand<A>, Operand<A>),
    /// sub {0}, {1}
    SUB(Operand<A>, Operand<A>),
    /// or {0}, {1},
    OR(Operand<A>, Operand<A>),
    /// xor {0}, {1},
    XOR(Operand<A>, Operand<A>),
    /// and {0}, {1},
    AND(Operand<A>, Operand<A>),
    /// mul {0}, {1}, {2},
    MUL(Operand<A>, Operand<A>, Operand<A>),
    /// imul {0}, {1},
    IMUL(Operand<A>, Operand<A>),
    /// imul1 {0}, {1}, {2} (one-operand form has 128 bit output)
    IMUL1(Operand<A>, Operand<A>, Operand<A>),
    /// idiv {0}, {1}, {2}
    IDIV(Operand<A>, Operand<A>, Operand<A>),
    /// not {0}
    NOT(Operand<A>),
    /// neg {0}
    NEG(Operand<A>),
    /// bextr {0}, {1}, {2}
    BEXTR(Operand<A>, Operand<A>, Operand<A>),
    /// jmp {0}
    JMP(Operand<A>),
    /// push {0}
    PUSH(Operand<A>),
    /// pop {0}
    POP(Operand<A>),
    /// ret
    RET,
    /// test {0}, {1}
    TEST(Operand<A>, Operand<A>),
    /// cmp {0}, {1}
    CMP(Operand<A>, Operand<A>),

    /// sets {0}
    SETS(Operand<A>), //n
    /// sete {0}
    SETE(Operand<A>), //z
    /// setc {0}
    SETC(Operand<A>), //c
    /// seto {0}
    SETO(Operand<A>), //v

    /// setne {0}
    SETNE(Operand<A>),
    /// setnz {0}
    SETNZ(Operand<A>),
    /// setb {0}
    SETB(Operand<A>),
    /// setbe {0}
    SETBE(Operand<A>),
    /// seta {0}
    SETA(Operand<A>),
    /// setg {0}
    SETG(Operand<A>),
    /// setge {0}
    SETGE(Operand<A>),
    /// setl {0}
    SETL(Operand<A>),
    /// setle {0}
    SETLE(Operand<A>),
    /// setae {0}
    SETAE(Operand<A>),
    /// je {0}
    JE(Operand<A>),
    /// jne {0}
    JNE(Operand<A>),
    /// nop
    NOP,
    /// int {0}
    INT(Operand<A>),

    /// out {0} {1}
    OUT(Operand<A>, Operand<A>),

    /// dead instruction
    DEAD,

    /// call {function}
    CALL {
        function: Operand<A>,
        nr_input_args: usize,
        nr_output_args: usize,
    },
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

#[derive(Clone, Copy)]
#[derive_where(PartialEq, Eq)]
pub enum OperandKind<A: Alloc> {
    Immediate(u64),
    Memory {
        base: Option<Register>,
        index: Option<Register>,
        scale: MemoryScale,
        displacement: i32,
        segment_override: Option<SegmentRegister>,
    },
    Register(Register),
    Target(Ref<X86Block<A>>),
}

impl<A: Alloc> Display for Operand<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}:{}", self.kind, self.width_in_bits)
    }
}

impl<A: Alloc> Display for OperandKind<A> {
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

impl<A: Alloc> Debug for OperandKind<A> {
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

#[derive(Debug, Clone, Copy)]
#[derive_where(PartialEq, Eq)]
pub struct Operand<A: Alloc> {
    pub kind: OperandKind<A>,
    pub width_in_bits: Width,
}

impl<A: Alloc> Operand<A> {
    pub fn kind(&self) -> &OperandKind<A> {
        &self.kind
    }

    pub fn width(&self) -> Width {
        self.width_in_bits
    }

    pub fn imm(width_in_bits: Width, value: u64) -> Operand<A> {
        Operand {
            kind: OperandKind::Immediate(value),
            width_in_bits: (width_in_bits),
        }
    }

    pub fn preg(width_in_bits: Width, reg: PhysicalRegister) -> Operand<A> {
        Operand {
            kind: OperandKind::Register(Register::Physical(reg)),
            width_in_bits: (width_in_bits),
        }
    }

    pub fn vreg(width_in_bits: Width, reg: usize) -> Operand<A> {
        Operand {
            kind: OperandKind::Register(Register::Virtual(reg)),
            width_in_bits: (width_in_bits),
        }
    }

    pub fn greg(width_in_bits: Width, reg: usize) -> Operand<A> {
        Operand {
            kind: OperandKind::Register(Register::Global(reg)),
            width_in_bits: (width_in_bits),
        }
    }

    pub fn mem_base(width_in_bits: Width, base: Register) -> Operand<A> {
        Self::mem_base_displ(width_in_bits, base, 0)
    }

    pub fn mem_base_displ(width_in_bits: Width, base: Register, displacement: i32) -> Operand<A> {
        Operand {
            kind: OperandKind::Memory {
                base: Some(base),
                index: None,
                scale: MemoryScale::S1,
                displacement,
                segment_override: None,
            },
            width_in_bits: (width_in_bits),
        }
    }

    pub fn mem_base_idx_scale(
        width_in_bits: Width,
        base: Register,
        idx: Register,
        scale: MemoryScale,
    ) -> Operand<A> {
        Self::mem_base_idx_scale_displ(width_in_bits, base, idx, scale, 0)
    }

    pub fn mem_base_idx_scale_displ(
        width_in_bits: Width,
        base: Register,
        idx: Register,
        scale: MemoryScale,
        displacement: i32,
    ) -> Operand<A> {
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
        width_in_bits: u16,
        segment: SegmentRegister,
        displacement: i32,
    ) -> Operand<A> {
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

    pub fn target(target: Ref<X86Block<A>>) -> Self {
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
pub struct Instruction<A: Alloc>(pub Opcode<A>);

macro_rules! alu_op {
    ($gen_name: ident, $opcode: ident) => {
        pub fn $gen_name(src: Operand<A>, dst: Operand<A>) -> Self {
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

impl<A: Alloc> Display for Instruction<A> {
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

impl<A: Alloc> Instruction<A> {
    pub fn adc(a: Operand<A>, b: Operand<A>, c: Operand<A>) -> Self {
        Self(Opcode::ADC(a, b, c))
    }

    pub fn mov(src: Operand<A>, dst: Operand<A>) -> Result<Self, ()> {
        if src.width() != dst.width() {
            return Err(());
        }
        if let OperandKind::Immediate(_) = src.kind()
            && src.width() == Width::_128
        {
            return Err(());
        }
        Ok(Self(Opcode::MOV(src, dst)))
    }

    pub fn movzx(src: Operand<A>, dst: Operand<A>) -> Self {
        assert!(
            src.width() < dst.width(),
            "can't zero extend {src} to {dst}"
        );
        Self(Opcode::MOVZX(src, dst))
    }

    pub fn movsx(src: Operand<A>, dst: Operand<A>) -> Self {
        assert!(
            src.width() < dst.width(),
            "can't sign extend {src} to {dst}"
        );
        Self(Opcode::MOVSX(src, dst))
    }

    pub fn lea(src: Operand<A>, dst: Operand<A>) -> Self {
        Self(Opcode::LEA(src, dst))
    }

    pub fn and(src: Operand<A>, dst: Operand<A>) -> Self {
        Self(Opcode::AND(src, dst))
    }

    pub fn mul(src: Operand<A>, dst_lo: Operand<A>, dst_hi: Operand<A>) -> Self {
        Self(Opcode::MUL(src, dst_lo, dst_hi))
    }

    pub fn imul(src: Operand<A>, dst: Operand<A>) -> Self {
        Self(Opcode::IMUL(src, dst))
    }

    pub fn imul1(src: Operand<A>, dst_lo: Operand<A>, dst_hi: Operand<A>) -> Self {
        Self(Opcode::IMUL1(src, dst_lo, dst_hi))
    }

    pub fn idiv(dividend_hi: Operand<A>, dividend_lo: Operand<A>, divisor: Operand<A>) -> Self {
        Self(Opcode::IDIV(dividend_hi, dividend_lo, divisor))
    }

    pub fn shl(amount: Operand<A>, op0: Operand<A>) -> Self {
        Self(Opcode::SHL(amount, op0))
    }

    pub fn shr(amount: Operand<A>, op0: Operand<A>) -> Self {
        Self(Opcode::SHR(amount, op0))
    }

    pub fn sar(amount: Operand<A>, op0: Operand<A>) -> Self {
        Self(Opcode::SAR(amount, op0))
    }

    pub fn bextr(ctrl: Operand<A>, src: Operand<A>, dst: Operand<A>) -> Self {
        Self(Opcode::BEXTR(ctrl, src, dst))
    }

    pub fn jmp(block: Ref<X86Block<A>>) -> Self {
        Self(Opcode::JMP(Operand::target(block)))
    }

    pub fn push(src: Operand<A>) -> Self {
        Self(Opcode::PUSH(src))
    }

    pub fn pop(dest: Operand<A>) -> Self {
        Self(Opcode::POP(dest))
    }

    pub fn ret() -> Self {
        Self(Opcode::RET)
    }

    pub fn nop() -> Self {
        Self(Opcode::NOP)
    }

    pub fn test(op0: Operand<A>, op1: Operand<A>) -> Self {
        Self(Opcode::TEST(op0, op1))
    }

    pub fn cmp(op0: Operand<A>, op1: Operand<A>) -> Self {
        Self(Opcode::CMP(op0, op1))
    }
    pub fn seto(r: Operand<A>) -> Self {
        Self(Opcode::SETO(r))
    }
    pub fn setc(r: Operand<A>) -> Self {
        Self(Opcode::SETC(r))
    }
    pub fn sete(r: Operand<A>) -> Self {
        Self(Opcode::SETE(r))
    }

    pub fn sets(r: Operand<A>) -> Self {
        Self(Opcode::SETS(r))
    }

    pub fn setne(r: Operand<A>) -> Self {
        Self(Opcode::SETNE(r))
    }
    pub fn setnz(r: Operand<A>) -> Self {
        Self(Opcode::SETNZ(r))
    }

    pub fn setb(r: Operand<A>) -> Self {
        Self(Opcode::SETB(r))
    }
    pub fn setl(r: Operand<A>) -> Self {
        Self(Opcode::SETL(r))
    }

    pub fn setle(r: Operand<A>) -> Self {
        Self(Opcode::SETLE(r))
    }
    pub fn setge(r: Operand<A>) -> Self {
        Self(Opcode::SETGE(r))
    }
    pub fn setg(r: Operand<A>) -> Self {
        Self(Opcode::SETG(r))
    }
    pub fn setbe(r: Operand<A>) -> Self {
        Self(Opcode::SETBE(r))
    }

    pub fn seta(r: Operand<A>) -> Self {
        Self(Opcode::SETA(r))
    }
    pub fn setae(r: Operand<A>) -> Self {
        Self(Opcode::SETAE(r))
    }

    pub fn je(block: Ref<X86Block<A>>) -> Self {
        Self(Opcode::JE(Operand::target(block)))
    }

    pub fn jne(block: Ref<X86Block<A>>) -> Self {
        Self(Opcode::JNE(Operand::target(block)))
    }

    pub fn out(port: Operand<A>, value: Operand<A>) -> Self {
        Self(Opcode::OUT(port, value))
    }

    pub fn not(r: Operand<A>) -> Self {
        Self(Opcode::NOT(r))
    }

    pub fn neg(r: Operand<A>) -> Self {
        Self(Opcode::NEG(r))
    }

    pub fn int(n: Operand<A>) -> Self {
        Self(Opcode::INT(n))
    }

    pub fn cmove(src: Operand<A>, dest: Operand<A>) -> Self {
        Self(Opcode::CMOVE(src, dest))
    }

    pub fn cmovne(src: Operand<A>, dest: Operand<A>) -> Self {
        Self(Opcode::CMOVNE(src, dest))
    }

    pub fn call(function: Operand<A>, nr_input_args: usize, nr_output_args: usize) -> Self {
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
        label_map: &HashMapA<Ref<X86Block<A>>, CodeLabel, A>,
    ) {
        use {
            Opcode::*,
            OperandKind::{Immediate as I, Memory as M, Register as R, Target as T},
            Register::Physical as PHYS,
        };

        match &self.0 {
            // do not emit dead instructions
            DEAD => (),
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

            // control flow
            JNE(Operand {
                kind: T(target), ..
            }) => {
                let label = label_map
                    .get(target)
                    .unwrap_or_else(|| panic!("no label for {target:?} found"))
                    .clone();
                assembler.jne(label).unwrap();
            }
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
            NOT(Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_64,
            }) => assembler.not::<AsmRegister64>(value.into()).unwrap(),
            NOT(Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_32,
            }) => assembler.not::<AsmRegister32>(value.into()).unwrap(),
            NOT(Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_16,
            }) => assembler.not::<AsmRegister16>(value.into()).unwrap(),
            NOT(Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_8,
            }) => assembler.not::<AsmRegister8>(value.into()).unwrap(),
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
            IDIV(
                Operand {
                    kind: R(PHYS(hi)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(lo)),
                    width_in_bits: Width::_64,
                },
                Operand {
                    kind: R(PHYS(div)),
                    width_in_bits: Width::_64,
                },
            ) => {
                assert_eq!(*hi, PhysicalRegister::RDX);
                assert_eq!(*lo, PhysicalRegister::RAX);
                assembler.idiv::<AsmRegister64>(div.into()).unwrap();
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
            _ => panic!("cannot encode this instruction {}", self),
        }
    }

    pub fn get_operands_mut(
        &mut self,
    ) -> impl Iterator<Item = Option<(OperandDirection, &mut Operand<A>)>> + '_ {
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
            | Opcode::IMUL(src, dst) => [
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
            Opcode::IDIV(dividend_hi, dividend_lo, divisor) => [
                Some((OperandDirection::InOut, dividend_hi)),
                Some((OperandDirection::InOut, dividend_lo)),
                Some((OperandDirection::In, divisor)),
            ]
            .into_iter(),
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
            Opcode::PUSH(src) => [Some((OperandDirection::In, src)), None, None].into_iter(),
            Opcode::POP(dest) => [Some((OperandDirection::Out, dest)), None, None].into_iter(),
            Opcode::DEAD => panic!(),
            Opcode::OUT(port, value) => [
                Some((OperandDirection::In, port)),
                Some((OperandDirection::In, value)),
                None,
            ]
            .into_iter(),
        }
    }

    pub fn get_operands_copy(&self) -> Vec<(OperandDirection, Operand<A>)> {
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
            | Opcode::IMUL(src, dst) => {
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
            Opcode::IDIV(dividend_hi, dividend_lo, divisor) => [
                (OperandDirection::InOut, dividend_hi),
                (OperandDirection::InOut, dividend_lo),
                (OperandDirection::In, divisor),
            ]
            .into_iter()
            .collect(),
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
            Opcode::DEAD => panic!(),
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
        }
    }

    pub fn get_use_defs(&self) -> impl Iterator<Item = (UseDef, Width)> + '_ {
        self.get_operands_copy()
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

    pub fn get_use_defs_mut(&mut self) -> impl Iterator<Item = UseDefMut> + '_ {
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
