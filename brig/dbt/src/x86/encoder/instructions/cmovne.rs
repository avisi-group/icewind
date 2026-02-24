use {
    crate::x86::encoder::{
        Operand,
        OperandKind::{Immediate as I, Register as R},
        Width,
        registers::Register::Physical as PHYS,
    },
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, AsmRegisterXmm, CodeAssembler
    },
};

pub fn encode(assembler: &mut CodeAssembler, src: &Operand, dst: &Operand) {
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
                .cmovne::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
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
                .cmovne::<AsmRegister32, AsmRegister32>(dst.into(), src.into())
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
                .cmovne::<AsmRegister16, AsmRegister16>(dst.into(), src.into())
                .unwrap();
        }
        _ => todo!("xor {src} {dst}"),
    }
}
