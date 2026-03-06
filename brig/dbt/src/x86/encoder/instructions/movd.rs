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
                width_in_bits: Width::_16,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_16,
            },
        ) => {
            //assert_eq!(src_width_in_bits, dst_width_in_bits);
            if src.is_xmm() {
                assembler
                    .movd::<AsmRegister32, AsmRegisterXmm>(
                        dst.try_into().unwrap(),
                        src.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                assembler
                    .movd::<AsmRegisterXmm, AsmRegister32>(
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
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
        ) => {
            //assert_eq!(src_width_in_bits, dst_width_in_bits);
            if src.is_xmm() {
                assembler
                    .movd::<AsmRegister32, AsmRegisterXmm>(
                        dst.try_into().unwrap(),
                        src.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                assembler
                    .movd::<AsmRegisterXmm, AsmRegister32>(
                        dst.try_into().unwrap(),
                        src.try_into().unwrap(),
                    )
                    .unwrap();
            }
        }

        _ => todo!("movd {src} {dst}"),
    }
}
