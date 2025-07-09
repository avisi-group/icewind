use {
    crate::host::dbt::{
        Alloc,
        x86::encoder::{
            Operand,
            OperandKind::{Immediate as I, Register as R},
            Width,
            registers::Register::Physical as PHYS,
        },
    },
    iced_x86::code_asm::{AsmRegister8, AsmRegister32, AsmRegister64, CodeAssembler},
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, src: &Operand<A>, dst: &Operand<A>) {
    match (src, dst) {
        // SUB IMM -> R
        (
            Operand {
                kind: I(src),
                width_in_bits: _,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
        ) => {
            assembler
                .sub::<AsmRegister64, i32>(dst.into(), i32::try_from(*src).unwrap())
                .unwrap();
        }
        // SUB IMM -> R
        (
            Operand {
                kind: I(src),
                width_in_bits: _,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .sub::<AsmRegister32, i32>(dst.into(), i32::try_from(*src).unwrap())
                .unwrap();
        }
        // SUB IMM -> R: todo remove me
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_8,
            },
        ) => {
            assembler
                .sub::<AsmRegister8, i32>(dst.into(), i32::try_from(*src).unwrap())
                .unwrap();
        }
        // SUB R -> R
        (
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
                .sub::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
                .unwrap();
        }
        // SUB IMM -> R
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_8,
            },
        ) => {
            assembler
                .sub::<AsmRegister8, i32>(dst.into(), i32::try_from(*src).unwrap())
                .unwrap();
        }
        _ => todo!("sub {src} {dst}"),
    }
}
