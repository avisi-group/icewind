use {
    crate::host::dbt::{
        Alloc,
        x86::encoder::{
            Operand, OperandKind::Register as R, Width, registers::Register::Physical as PHYS,
        },
    },
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, CodeAssembler,
    },
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, src: &Operand<A>, dst: &Operand<A>) {
    match (src, dst) {
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
            (Width::_32, Width::_64) => assembler
                .movsxd::<AsmRegister64, AsmRegister32>(dst.into(), src.into())
                .unwrap(),
            (Width::_16, Width::_32) => assembler
                .movsx::<AsmRegister32, AsmRegister16>(dst.into(), src.into())
                .unwrap(),
            (Width::_16, Width::_64) => assembler
                .movsx::<AsmRegister64, AsmRegister16>(dst.into(), src.into())
                .unwrap(),
            (Width::_8, Width::_64) => assembler
                .movsx::<AsmRegister64, AsmRegister8>(dst.into(), src.into())
                .unwrap(),
            (Width::_8, Width::_32) => assembler
                .movsx::<AsmRegister32, AsmRegister8>(dst.into(), src.into())
                .unwrap(),
            (src, dst) => todo!("{src} -> {dst} sign extend mov not implemented"),
        },
        _ => todo!("movsx {src} {dst}"),
    }
}
