use {
    crate::x86::encoder::{
        Operand,
        OperandKind::{Immediate as I, Register as R},
        Width,
        registers::Register::Physical as PHYS,
    },
    iced_x86::code_asm::{AsmRegister8, AsmRegister32, AsmRegister64, CodeAssembler},
    shared::Alloc,
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, src: &Operand<A>, dst: &Operand<A>) {
    match (src, dst) {
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
                .xor::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
                .unwrap();
        }
        (
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
                .xor::<AsmRegister32, AsmRegister32>(dst.into(), src.into())
                .unwrap();
        }
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_8,
            },
        ) => {
            assembler
                .xor::<AsmRegister8, AsmRegister8>(dst.into(), src.into())
                .unwrap();
        }
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
        ) => {
            assembler
                .xor::<AsmRegister64, i32>(dst.into(), (*src).try_into().unwrap())
                .unwrap();
        }
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .xor::<AsmRegister32, u32>(dst.into(), (*src).try_into().unwrap())
                .unwrap();
        }
        _ => todo!("xor {src} {dst}"),
    }
}
