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
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, AsmRegisterXmm, CodeAssembler,
    },
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, src: &Operand<A>, dst: &Operand<A>) {
    match (src, dst) {
        // MOVZX I -> R
        (
            Operand {
                kind: I(src),
                width_in_bits: src_width,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: dst_width,
            },
        ) => match (*src_width, *dst_width) {
            (Width::_8, Width::_32) => {
                assembler
                    .mov::<AsmRegister32, i32>(dst.into(), u8::try_from(*src).unwrap().into())
                    .unwrap();
            }
            (Width::_8, Width::_64) => {
                assembler
                    .mov::<AsmRegister32, i32>(dst.into(), u8::try_from(*src).unwrap().into())
                    .unwrap();
            }
            (Width::_16, Width::_64) => {
                assembler
                    .mov::<AsmRegister32, i32>(dst.into(), *src as i32)
                    .unwrap();
            }
            (Width::_16, Width::_32) => {
                assembler
                    .mov::<AsmRegister32, i32>(dst.into(), *src as i32)
                    .unwrap();
            }
            (Width::_32, Width::_64) => {
                assembler
                    .mov::<AsmRegister32, i32>(dst.into(), *src as i32)
                    .unwrap();
            }

            (src, dst) => todo!("{src} -> {dst} zero extend mov not implemented"),
        },
        // MOVZX R -> R
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: src_width,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: dst_width,
            },
        ) => match (*src_width, *dst_width) {
            (Width::_8, Width::_32) => assembler
                .movzx::<AsmRegister32, AsmRegister8>(dst.into(), src.into())
                .unwrap(),
            (Width::_8, Width::_64) => assembler
                .movzx::<AsmRegister64, AsmRegister8>(dst.into(), src.into())
                .unwrap(),
            (Width::_8, Width::_16) => assembler
                .movzx::<AsmRegister16, AsmRegister8>(dst.into(), src.into())
                .unwrap(),
            (Width::_16, Width::_32) => assembler
                .movzx::<AsmRegister32, AsmRegister16>(dst.into(), src.into())
                .unwrap(),
            (Width::_16, Width::_64) => assembler
                .movzx::<AsmRegister64, AsmRegister16>(dst.into(), src.into())
                .unwrap(),
            (Width::_32, Width::_64) => assembler
                .mov::<AsmRegister32, AsmRegister32>(dst.into(), src.into())
                .unwrap(),
            (Width::_64, Width::_128) => assembler
                .movq::<AsmRegisterXmm, AsmRegister64>(dst.into(), src.into())
                .unwrap(),

            (src, dst) => todo!("{src} -> {dst} zero extend mov not implemented"),
        },

        _ => todo!("movzx {src} {dst}"),
    }
}
