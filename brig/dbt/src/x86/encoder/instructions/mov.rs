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
        AsmRegisterXmm, CodeAssembler, asm_traits::CodeAsmMovq, byte_ptr, dword_ptr, qword_ptr,
        word_ptr, xmmword_ptr,
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
                panic!();
            } else {
                assembler
                    .mov::<AsmRegister64, AsmRegister64>(
                        dst.try_into().unwrap(),
                        src.try_into().unwrap(),
                    )
                    .unwrap();
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
                width_in_bits: Width::_128,
            },
        ) => {
            //assert_eq!(src_width_in_bits, dst_width_in_bits);

            assembler
                .movdqa::<AsmRegisterXmm, AsmRegisterXmm>(
                    dst.try_into().unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }

        // MOV R -> R
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
            //assert_eq!(src_width_in_bits, dst_width_in_bits);

            assembler
                .mov::<AsmRegister8, AsmRegister8>(dst.try_into().unwrap(), src.try_into().unwrap())
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
            if src.is_xmm() {
                assembler
                    .movq::<AsmRegisterXmm, AsmRegisterXmm>(
                        dst.try_into().unwrap(),
                        src.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                //assert_eq!(src_width_in_bits, dst_width_in_bits);
                assembler
                    .mov::<AsmRegister32, AsmRegister32>(
                        dst.try_into().unwrap(),
                        src.try_into().unwrap(),
                    )
                    .unwrap();
            }
        }

        // MOV R -> R
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
                .mov::<AsmRegister16, AsmRegister16>(
                    dst.try_into().unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
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
                panic!()
            } else {
                assembler
                    .mov::<AsmRegister64, AsmMemoryOperand>(
                        dst.try_into().unwrap(),
                        memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
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
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_8,
            },
        ) => {
            assembler
                .mov::<AsmRegister8, AsmMemoryOperand>(
                    dst.try_into().unwrap(),
                    memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                )
                .unwrap();
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
                width_in_bits: Width::_16,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_16,
            },
        ) => {
            assembler
                .mov::<AsmRegister16, AsmMemoryOperand>(
                    dst.try_into().unwrap(),
                    memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                )
                .unwrap();
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
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .mov::<AsmRegister32, AsmMemoryOperand>(
                    dst.try_into().unwrap(),
                    memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                )
                .unwrap();
        }
        // MOV I -> M
        (
            Operand {
                kind: I(imm),
                width_in_bits: Width::_32,
            },
            Operand {
                kind:
                    M {
                        base: None,
                        index,
                        scale,
                        displacement,
                        segment_override: Some(seg_reg),
                    },
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .mov::<AsmMemoryOperand, i32>(
                    dword_ptr(
                        segment_memory_operand_to_iced(*seg_reg, *index, *scale, *displacement)
                            .unwrap(),
                    ),
                    i32::try_from(*imm).unwrap(),
                )
                .unwrap();
        }
        // MOV M -> R
        (
            Operand {
                kind:
                    M {
                        base: None,
                        index,
                        scale,
                        displacement,
                        segment_override: Some(seg_reg),
                    },
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
        ) => {
            assembler
                .mov::<AsmRegister64, AsmMemoryOperand>(
                    dst.try_into().unwrap(),
                    segment_memory_operand_to_iced(*seg_reg, *index, *scale, *displacement)
                        .unwrap(),
                )
                .unwrap();
        }
        // MOV M -> R
        (
            Operand {
                kind:
                    M {
                        base: None,
                        index,
                        scale,
                        displacement,
                        segment_override: Some(seg_reg),
                    },
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .mov::<AsmRegister64, AsmMemoryOperand>(
                    dst.try_into().unwrap(),
                    segment_memory_operand_to_iced(*seg_reg, *index, *scale, *displacement)
                        .unwrap(),
                )
                .unwrap();
        }
        // MOV R -> M
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_8,
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
                width_in_bits: Width::_8,
            },
        ) => {
            assembler
                .mov::<AsmMemoryOperand, AsmRegister8>(
                    memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }
        // MOV R -> M
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_16,
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
                width_in_bits: Width::_16,
            },
        ) => {
            assembler
                .mov::<AsmMemoryOperand, AsmRegister16>(
                    memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }
        // MOV R -> M
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_32,
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
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .mov::<AsmMemoryOperand, AsmRegister32>(
                    memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }
        // MOV R -> M
        (
            Operand {
                kind: R(PHYS(src)),
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
            assembler
                .mov::<AsmMemoryOperand, AsmRegister64>(
                    memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }
        // MOV I -> M
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_32,
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
                width_in_bits: Width::_32,
            },
        ) => {
            // assert_eq!(src_width_in_bits, dst_width_in_bits);

            assembler
                .mov::<AsmMemoryOperand, u32>(
                    dword_ptr(
                        memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                    ),
                    *src as u32,
                )
                .unwrap();
        }
        // MOV I -> M
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_8,
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
                width_in_bits: Width::_8,
            },
        ) => {
            // assert_eq!(src_width_in_bits, dst_width_in_bits);

            assembler
                .mov::<AsmMemoryOperand, u32>(
                    byte_ptr(memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap()),
                    u32::try_from(*src).unwrap(),
                )
                .unwrap();
        }

        // MOV I -> M
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_16,
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
                width_in_bits: Width::_16,
            },
        ) => {
            // assert_eq!(src_width_in_bits, dst_width_in_bits);

            assembler
                .mov::<AsmMemoryOperand, u32>(
                    word_ptr(memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap()),
                    u32::try_from(*src).unwrap(),
                )
                .unwrap();
        }
        // MOV I -> M
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
            // lo
            assembler
                .mov::<AsmMemoryOperand, u32>(
                    dword_ptr(
                        memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                    ),
                    u32::try_from(*src & u64::from(u32::MAX)).unwrap(),
                )
                .unwrap();
            // hi
            assembler
                .mov::<AsmMemoryOperand, u32>(
                    dword_ptr(
                        memory_operand_to_iced(*base, *index, *scale, *displacement + 4).unwrap(),
                    ),
                    u32::try_from((*src >> 32) & u64::from(u32::MAX)).unwrap(),
                )
                .unwrap();
        }
        // MOV I -> R
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
            if *src == 0 {
                assembler
                    .xor::<AsmRegister8, AsmRegister8>(
                        dst.try_into().unwrap(),
                        dst.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                assembler
                    .mov::<AsmRegister8, i32>(dst.try_into().unwrap(), (*src).try_into().unwrap())
                    .unwrap();
            }
        }
        // MOV I -> R
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_16,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_16,
            },
        ) => {
            if *src == 0 {
                assembler
                    .xor::<AsmRegister16, AsmRegister16>(
                        dst.try_into().unwrap(),
                        dst.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                assembler
                    .mov::<AsmRegister16, i32>(dst.try_into().unwrap(), (*src).try_into().unwrap())
                    .unwrap();
            }
        }
        // MOV I -> R
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
            if dst.is_gpr() {
                if *src == 0 {
                    assembler
                        .xor::<AsmRegister32, AsmRegister32>(
                            dst.try_into().unwrap(),
                            dst.try_into().unwrap(),
                        )
                        .unwrap();
                } else if *src < i32::MAX as u64 {
                    assembler
                        .mov::<AsmRegister32, i32>(
                            dst.try_into().unwrap(),
                            i32::try_from(*src).unwrap(),
                        )
                        .unwrap();
                } else {
                    assembler
                        .mov::<AsmRegister64, u64>(dst.try_into().unwrap(), *src)
                        .unwrap();
                }
            } else {
                if *src == 0 {
                    assembler
                        .pxor::<AsmRegisterXmm, AsmRegisterXmm>(
                            dst.try_into().unwrap(),
                            dst.try_into().unwrap(),
                        )
                        .unwrap();
                } else {
                    panic!()
                }
            }
        }
        // MOV I -> R
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
            if dst.is_gpr() {
                if *src == 0 {
                    assembler
                        .xor::<AsmRegister32, AsmRegister32>(
                            dst.try_into().unwrap(),
                            dst.try_into().unwrap(),
                        )
                        .unwrap();
                } else {
                    assembler
                        .mov::<AsmRegister32, u32>(dst.try_into().unwrap(), *src as u32)
                        .unwrap();
                }
            } else {
                if *src == 0 {
                    assembler
                        .pxor::<AsmRegisterXmm, AsmRegisterXmm>(
                            dst.try_into().unwrap(),
                            dst.try_into().unwrap(),
                        )
                        .unwrap();
                } else {
                    todo!()
                }
            }
        }

        (
            // todo: fix this earlier in DBT
            Operand {
                kind: I(src),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
        ) => {
            // todo: maybe zero extend src here?
            if *src == 0 {
                assembler
                    .xor::<AsmRegister32, AsmRegister32>(
                        dst.try_into().unwrap(),
                        dst.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                assembler
                    .mov::<AsmRegister32, i32>(dst.try_into().unwrap(), (*src).try_into().unwrap())
                    .unwrap();
            }
        }
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
        ) => {
            // no need to write high bits
            assembler
                .mov::<AsmRegister32, i32>(dst.try_into().unwrap(), (*src).try_into().unwrap())
                .unwrap();
        }
        (
            // todo: fix this earlier in DBT
            Operand {
                kind: I(src),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
        ) => {
            // don't need to write high bits
            assembler
                .mov::<AsmRegister32, i32>(dst.try_into().unwrap(), (*src).try_into().unwrap())
                .unwrap();
        }
        (
            // todo: fix this earlier in DBT
            Operand {
                kind: I(src),
                width_in_bits: Width::_16,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
        ) => {
            // don't need to write high bits
            assembler
                .mov::<AsmRegister32, i32>(dst.try_into().unwrap(), (*src).try_into().unwrap())
                .unwrap();
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
                width_in_bits: Width::_128,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_128,
            },
        ) => {
            // todo: make this aligned!
            assembler
                .movdqu(
                    dst.try_into().unwrap(),
                    xmmword_ptr(
                        memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                    ),
                )
                .unwrap();
        }
        // MOV R -> M
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_128,
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
                width_in_bits: Width::_128,
            },
        ) => {
            // todo: make this aligned!
            assembler
                .movdqu(
                    xmmword_ptr(
                        memory_operand_to_iced(*base, *index, *scale, *displacement).unwrap(),
                    ),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }

        // MOV R -> R
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_128,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .movd::<AsmRegister32, AsmRegisterXmm>(
                    dst.try_into().unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
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
                .movq::<AsmRegisterXmm, AsmRegisterXmm>(
                    dst.try_into().unwrap(),
                    src.try_into().unwrap(),
                )
                .unwrap();
        }

        _ => todo!("mov {src} {dst}"),
    }
}
