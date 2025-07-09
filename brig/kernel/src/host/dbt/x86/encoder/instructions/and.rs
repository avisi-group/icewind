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
    iced_x86::code_asm::{
        AsmMemoryOperand, AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, CodeAssembler,
        byte_ptr, qword_ptr,
    },
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, src: &Operand<A>, dst: &Operand<A>) {
    match (src, dst) {
        (
            Operand { kind: I(left), .. },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_8,
            },
        ) => {
            if *left > i32::MAX as u64 {
                panic!("AND immediate too large: {left:x}");
            }
            assembler
                .and::<AsmRegister8, i32>(right.into(), *left as i32)
                .unwrap();
        }
        (
            Operand { kind: I(left), .. },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_16,
            },
        ) => {
            if *left > i32::MAX as u64 {
                panic!("AND immediate too large: {left:x}");
            }
            assembler
                .and::<AsmRegister16, i32>(right.into(), *left as i32)
                .unwrap();
        }
        (
            Operand { kind: I(left), .. },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_32,
            },
        ) => {
            if *left > i32::MAX as u64 {
                panic!("AND immediate too large: {left:x}");
            }
            assembler
                .and::<AsmRegister32, i32>(right.into(), *left as i32)
                .unwrap();
        }
        (
            Operand { kind: I(left), .. },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_64,
            },
        ) => {
            if *left > i32::MAX as u64 {
                panic!("AND immediate too large: {left:x}");
            }
            assembler
                .and::<AsmRegister64, i32>(right.into(), *left as i32)
                .unwrap();
        }
        // AND IMM -> MEM
        (
            Operand { kind: I(left), .. },
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
            if *left > i32::MAX as u64 {
                panic!("AND immediate too large: {left:x}");
            }
            assembler
                .and::<AsmMemoryOperand, i32>(
                    qword_ptr(memory_operand_to_iced(*base, *index, *scale, *displacement)),
                    *left as i32,
                )
                .unwrap();
        }
        // AND IMM -> MEM
        (
            Operand { kind: I(left), .. },
            Operand {
                kind:
                    M {
                        base: Some(PHYS(base)),
                        index,
                        scale,
                        displacement,
                        ..
                    },
                width_in_bits: Width::_8,
            },
        ) => {
            if *left > u8::MAX as u64 {
                panic!("AND immediate too large: {left:x}");
            }
            assembler
                .and::<AsmMemoryOperand, u32>(
                    byte_ptr(memory_operand_to_iced(*base, *index, *scale, *displacement)),
                    *left as u32,
                )
                .unwrap();
        }
        (
            Operand {
                kind: R(PHYS(left)),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_64,
            },
        ) => {
            assembler
                .and::<AsmRegister64, AsmRegister64>(right.into(), left.into())
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
                .and::<AsmRegister32, AsmRegister32>(right.into(), left.into())
                .unwrap();
        }
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
                .and::<AsmRegister8, AsmRegister8>(right.into(), left.into())
                .unwrap();
        }

        _ => todo!("and {src} {dst}"),
    }
}
