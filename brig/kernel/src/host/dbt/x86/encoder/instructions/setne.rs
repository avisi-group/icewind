use {
    crate::host::dbt::{
        Alloc,
        x86::encoder::{
            Operand, OperandKind::Register as R, Width, registers::Register::Physical as PHYS,
        },
    },
    iced_x86::code_asm::{AsmRegister8, AsmRegister32, CodeAssembler},
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, dst: &Operand<A>) {
    match dst {
        Operand {
            kind: R(PHYS(target)),
            width_in_bits: Width::_64,
        } => {
            assembler
                .xor::<AsmRegister32, AsmRegister32>(target.into(), target.into())
                .unwrap();
            assembler.setne::<AsmRegister8>(target.into()).unwrap();
        }
        Operand {
            kind: R(PHYS(target)),
            width_in_bits: Width::_8,
        } => {
            assembler.setne::<AsmRegister8>(target.into()).unwrap();
        }
        _ => todo!("setne {dst}"),
    }
}
