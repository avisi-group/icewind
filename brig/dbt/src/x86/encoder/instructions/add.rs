use {
    crate::x86::encoder::{
        Operand,
        OperandKind::{Immediate as I, Memory as M, Register as R},
        Width, memory_operand_to_iced,
        registers::Register::Physical as PHYS,
    },
    iced_x86::code_asm::{
        AsmMemoryOperand, AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, CodeAssembler,
        qword_ptr,
    },
};

pub fn encode(assembler: &mut CodeAssembler, src: &Operand, dst: &Operand) {
    match (src, dst) {
        // ADD R -> R
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
                .add::<AsmRegister64, AsmRegister64>(
                    dst.try_into().unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }
        // ADD R -> R
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
                .add::<AsmRegister32, AsmRegister32>(
                    dst.try_into().unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }
        // ADD R -> R
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_16,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_16,
            },
        ) => {
            assembler
                .add::<AsmRegister16, AsmRegister16>(
                    dst.try_into().unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }
        // ADD R -> R
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
                .add::<AsmRegister8, AsmRegister8>(dst.try_into().unwrap(), src.try_into().unwrap())
                .unwrap();
        }
        // ADD IMM -> R
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
                .add::<AsmRegister64, i32>(
                    dst.try_into().unwrap(),
                    i32::try_from(*src as i64).unwrap(),
                )
                .unwrap();
        }
        // ADD IMM -> R
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
                .add::<AsmRegister32, i32>(
                    dst.try_into().unwrap(),
                    i32::try_from(*src as i64).unwrap(),
                )
                .unwrap();
        }
        // ADD IMM -> M
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_64,
            },
            Operand {
                kind:
                    M {
                        base: Some(PHYS(base)),
                        index,
                        scale,
                        displacement,
                        ..
                    },
                width_in_bits: Width::_64,
            },
        ) => {
            assert!(*src < i32::MAX as u64);

            assembler
                .add::<AsmMemoryOperand, i32>(
                    qword_ptr(
                        memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                    ),
                    i32::try_from(*src as i64).unwrap(),
                )
                .unwrap();
        }

        _ => todo!("add {src} {dst}"),
    }
}
