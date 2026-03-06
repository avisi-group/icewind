use {
    crate::x86::encoder::{
        Operand, OperandKind::Register as R, Width, registers::Register::Physical as PHYS,
    },
    iced_x86::code_asm::{AsmRegister8, AsmRegister32, CodeAssembler},
};

pub fn encode(assembler: &mut CodeAssembler, dst: &Operand) {
    match dst {
        Operand {
            kind: R(PHYS(target)),
            width_in_bits: Width::_64,
        } => {
            assembler
                .xor::<AsmRegister32, AsmRegister32>(
                    target.try_into().unwrap(),
                    target.try_into().unwrap(),
                )
                .unwrap();
            assembler
                .setne::<AsmRegister8>(target.try_into().unwrap())
                .unwrap();
        }
        Operand {
            kind: R(PHYS(target)),
            width_in_bits: Width::_8,
        } => {
            assembler
                .setne::<AsmRegister8>(target.try_into().unwrap())
                .unwrap();
        }
        _ => todo!("setne {dst}"),
    }
}
