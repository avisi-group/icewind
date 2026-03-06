use {
    crate::x86::encoder::{
        Operand,
        OperandKind::{Immediate as I, Memory as M, Register as R},
        Width, memory_operand_to_iced,
        registers::Register::Physical as PHYS,
        segment_memory_operand_to_iced,
    },
    iced_x86::code_asm::{
        AsmMemoryOperand, AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64,
        AsmRegisterXmm, CodeAssembler, byte_ptr, dword_ptr, qword_ptr, word_ptr, xmmword_ptr,
    },
};

pub fn encode(assembler: &mut CodeAssembler, src: &Operand, dst: &Operand) {
    match (src, dst) {
        // MOV R -> R
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
            //assert_eq!(src_width_in_bits, dst_width_in_bits);
            if src.is_xmm() {
                assembler
                    .movq::<AsmRegister64, AsmRegisterXmm>(
                        dst.try_into().unwrap(),
                        src.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                assembler
                    .movq::<AsmRegisterXmm, AsmRegister64>(
                        dst.try_into().unwrap(),
                        src.try_into().unwrap(),
                    )
                    .unwrap();
            }
        }
        // MOV M -> R
        (
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
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
        ) => {
            if dst.is_xmm() {
                assembler
                    .movq::<AsmRegisterXmm, AsmMemoryOperand>(
                        dst.try_into().unwrap(),
                        qword_ptr(
                            memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                        ),
                    )
                    .unwrap();
            } else {
                panic!()
            }
        }

        // MOV R -> R
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_128,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
        ) => {
            assembler
                .movq::<AsmRegister64, AsmRegisterXmm>(
                    dst.try_into().unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }

        // MOV R -> R
        (
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
                .movq::<AsmRegisterXmm, AsmRegister64>(
                    dst.try_into().unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }

        // MOV R -> R
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
            if src.is_gpr() && dst.is_xmm() {
                assembler
                    .movq::<AsmRegisterXmm, AsmRegister64>(
                        dst.try_into().unwrap(),
                        src.try_into().unwrap(),
                    )
                    .unwrap();
            } else if src.is_xmm() && dst.is_gpr() {
                assembler
                    .movq::<AsmRegister64, AsmRegisterXmm>(
                        dst.try_into().unwrap(),
                        src.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                panic!()
            }
        }

        _ => todo!("movq {src} {dst}"),
    }
}
