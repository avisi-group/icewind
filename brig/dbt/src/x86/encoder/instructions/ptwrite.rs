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
};

pub fn encode(assembler: &mut CodeAssembler, data: &Operand) {
    match data {
        Operand {
            kind: R(PHYS(data)),
            width_in_bits: Width::_64,
        } => assembler
            .ptwrite::<AsmRegister64>(data.try_into().unwrap())
            .unwrap(),
        _ => todo!(),
    }
}
