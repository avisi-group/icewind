use {
    crate::host::dbt::{
        Alloc,
        x86::encoder::{
            Operand,
            OperandKind::{Immediate as I, Memory as M, Register as R},
            Width, memory_operand_to_iced,
            registers::Register::Physical as PHYS,
        },
    },
    iced_x86::code_asm::{AsmMemoryOperand, AsmRegister64, CodeAssembler, qword_ptr},
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, src: &Operand<A>, dst: &Operand<A>) {
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
                .add::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
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
                .add::<AsmRegister64, i32>(dst.into(), i32::try_from(*src as i64).unwrap())
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
                    qword_ptr(memory_operand_to_iced(*base, *index, *scale, *displacement)),
                    i32::try_from(*src as i64).unwrap(),
                )
                .unwrap();
        }
        _ => todo!("add {src} {dst}"),
    }
}
