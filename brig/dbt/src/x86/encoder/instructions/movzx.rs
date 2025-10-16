use {
    crate::x86::encoder::{
        Operand,
        OperandKind::{Immediate as I, Register as R},
        Width,
        registers::Register::Physical as PHYS,
    },
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, AsmRegisterXmm, CodeAssembler,
    },
 brig_common::Alloc,
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, src: &Operand<A>, dst: &Operand<A>) {
    match (src, dst) {
        // MOVZX I -> XMM
        (
            Operand {
                kind: I(0),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_128,
            },
        ) => assembler
            .pxor::<AsmRegisterXmm, AsmRegisterXmm>(
                dst.try_into().unwrap(),
                dst.try_into().unwrap(),
            )
            .unwrap(),

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
            (_, _) => todo!("{src} ({src_width}) -> {dst} zero extend mov not implemented"),
        },
        // MOVZX R -> R
        (
            Operand {
                kind: R(PHYS(src_r)),
                width_in_bits: src_width,
            },
            Operand {
                kind: R(PHYS(dst_r)),
                width_in_bits: dst_width,
            },
        ) => match (*src_width, *dst_width) {
            (Width::_8, Width::_32) => assembler
                .movzx::<AsmRegister32, AsmRegister8>(dst_r.into(), src_r.into())
                .unwrap(),
            (Width::_8, Width::_64) => assembler
                .movzx::<AsmRegister64, AsmRegister8>(dst_r.into(), src_r.into())
                .unwrap(),
            (Width::_8, Width::_16) => assembler
                .movzx::<AsmRegister16, AsmRegister8>(dst_r.into(), src_r.into())
                .unwrap(),
            (Width::_16, Width::_32) => assembler
                .movzx::<AsmRegister32, AsmRegister16>(dst_r.into(), src_r.into())
                .unwrap(),
            (Width::_16, Width::_64) => assembler
                .movzx::<AsmRegister64, AsmRegister16>(dst_r.into(), src_r.into())
                .unwrap(),
            (Width::_32, Width::_64) => assembler
                .mov::<AsmRegister32, AsmRegister32>(dst_r.into(), src_r.into())
                .unwrap(),
            (Width::_64, Width::_128) => assembler
                .movq::<AsmRegisterXmm, AsmRegister64>(dst_r.try_into().unwrap(), src_r.into())
                .unwrap(),

            (Width::_8, Width::_128) => {
                assembler
                    .movzx::<AsmRegister64, AsmRegister8>(src_r.into(), src_r.into())
                    .unwrap();
                assembler
                    .movq::<AsmRegisterXmm, AsmRegister64>(dst_r.try_into().unwrap(), src_r.into())
                    .unwrap()
            }

            (Width::_16, Width::_128) => {
                assembler
                    .movzx::<AsmRegister64, AsmRegister16>(src_r.into(), src_r.into())
                    .unwrap();
                assembler
                    .movq::<AsmRegisterXmm, AsmRegister64>(dst_r.try_into().unwrap(), src_r.into())
                    .unwrap()
            }

            (Width::_32, Width::_128) => {
                assembler
                    .mov::<AsmRegister32, AsmRegister32>(src_r.into(), src_r.into())
                    .unwrap();
                assembler
                    .movq::<AsmRegisterXmm, AsmRegister64>(dst_r.try_into().unwrap(), src_r.into())
                    .unwrap()
            }
            (_, _) => todo!("{src} -> {dst} zero extend mov not implemented"),
        },

        _ => todo!("movzx {src} {dst}"),
    }
}
