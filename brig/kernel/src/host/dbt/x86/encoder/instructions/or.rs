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
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, CodeAssembler,
    },
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, src: &Operand<A>, dst: &Operand<A>) {
    match (src, dst) {
        // OR I R
        (
            Operand {
                kind: I(left),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_8,
            },
        ) => {
            assembler
                .or::<AsmRegister8, i32>(right.into(), i32::try_from(*left).unwrap())
                .unwrap();
        }
        // OR I R
        (
            Operand {
                kind: I(left),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .or::<AsmRegister32, i32>(right.into(), i32::try_from(*left).unwrap())
                .unwrap();
        }
        // OR I R
        (
            Operand {
                kind: I(left),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_32,
            },
        ) => {
            if *left < i32::MAX as u64 {
                assembler
                    .or::<AsmRegister32, i32>(right.into(), *left as i32)
                    .unwrap();
            } else {
                panic!("{left:?}")
            }
        }
        // OR I R
        (
            Operand {
                kind: I(left),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_64,
            },
        ) => {
            if *left < i32::MAX as u64 {
                assembler
                    .or::<AsmRegister64, i32>(right.into(), *left as i32)
                    .unwrap();
            } else {
                panic!("{left:?}")
            }
        }
        // OR R R
        (
            Operand {
                kind: R(PHYS(left)),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_8,
            },
        ) => {
            assembler
                .or::<AsmRegister8, AsmRegister8>(right.into(), left.into())
                .unwrap();
        }
        (
            Operand {
                kind: R(PHYS(left)),
                width_in_bits: Width::_16,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_16,
            },
        ) => {
            assembler
                .or::<AsmRegister16, AsmRegister16>(right.into(), left.into())
                .unwrap();
        }
        (
            Operand {
                kind: R(PHYS(left)),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .or::<AsmRegister32, AsmRegister32>(right.into(), left.into())
                .unwrap();
        }
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
                .or::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
                .unwrap();
        }
        _ => todo!("or {src} {dst}"),
    }
}
