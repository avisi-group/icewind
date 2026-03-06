use {
    crate::x86::encoder::{
        Operand, OperandKind::Register as R, Width, registers::Register::Physical as PHYS,
    },
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, AsmRegisterXmm, CodeAssembler,
    },
};

pub fn encode(assembler: &mut CodeAssembler, dst: &Operand) {
    match dst {
        Operand {
            kind: R(PHYS(value)),
            width_in_bits: Width::_128,
        } => {
            // https://stackoverflow.com/a/52472340/8070904
            // TODO: we only allocate up to XMM7, fix this if we ever need to allocate more
            // xmm registers
            assembler
                .pcmpeqd::<AsmRegisterXmm, AsmRegisterXmm>(
                    iced_x86::code_asm::registers::xmm8,
                    iced_x86::code_asm::registers::xmm8,
                )
                .unwrap();
            assembler
                .pxor::<AsmRegisterXmm, AsmRegisterXmm>(
                    value.try_into().unwrap(),
                    iced_x86::code_asm::registers::xmm8,
                )
                .unwrap()
        }
        Operand {
            kind: R(PHYS(value)),
            width_in_bits: Width::_64,
        } => assembler
            .not::<AsmRegister64>(value.try_into().unwrap())
            .unwrap(),
        Operand {
            kind: R(PHYS(value)),
            width_in_bits: Width::_32,
        } => assembler
            .not::<AsmRegister32>(value.try_into().unwrap())
            .unwrap(),
        Operand {
            kind: R(PHYS(value)),
            width_in_bits: Width::_16,
        } => assembler
            .not::<AsmRegister16>(value.try_into().unwrap())
            .unwrap(),
        Operand {
            kind: R(PHYS(value)),
            width_in_bits: Width::_8,
        } => assembler
            .not::<AsmRegister8>(value.try_into().unwrap())
            .unwrap(),
        _ => todo!("not {dst}"),
    }
}
