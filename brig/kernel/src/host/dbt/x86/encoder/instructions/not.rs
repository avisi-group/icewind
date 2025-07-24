use {
    crate::host::dbt::{
        Alloc,
        x86::encoder::{
            Operand,
            OperandKind::{Immediate as I, Register as R},
            Width,
            registers::{
                PhysicalRegister::{General as G, Xmm as X},
                PhysicalRegisterXmm,
                Register::Physical as PHYS,
            },
        },
    },
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, AsmRegisterXmm, CodeAssembler,
    },
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, dst: &Operand<A>) {
    match dst {
        Operand {
            kind: R(PHYS(X(value))),
            width_in_bits: Width::_128,
        } => {
            // https://stackoverflow.com/a/52472340/8070904
            // TODO: we only allocate up to XMM7, fix this if we ever need to allocate more
            // xmm registers
            assembler
                .pcmpeqd::<AsmRegisterXmm, AsmRegisterXmm>(
                    iced_x86::code_asm::registers::xmm8,
                    iced_x86::code_asm::registers::xmm8,
                )
                .unwrap();
            assembler
                .pxor::<AsmRegisterXmm, AsmRegisterXmm>(
                    value.into(),
                    iced_x86::code_asm::registers::xmm8,
                )
                .unwrap()
        }
        Operand {
            kind: R(PHYS(value)),
            width_in_bits: Width::_64,
        } => assembler.not::<AsmRegister64>(value.into()).unwrap(),
        Operand {
            kind: R(PHYS(value)),
            width_in_bits: Width::_32,
        } => assembler.not::<AsmRegister32>(value.into()).unwrap(),
        Operand {
            kind: R(PHYS(value)),
            width_in_bits: Width::_16,
        } => assembler.not::<AsmRegister16>(value.into()).unwrap(),
        Operand {
            kind: R(PHYS(value)),
            width_in_bits: Width::_8,
        } => assembler.not::<AsmRegister8>(value.into()).unwrap(),
        _ => todo!("not {dst}"),
    }
}
