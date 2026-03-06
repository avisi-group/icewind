use {
    crate::x86::encoder::{
        Operand,
        OperandKind::{Immediate as I, Register as R},
        Width,
        registers::{PhysicalRegister, Register::Physical as PHYS},
    },
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, AsmRegisterXmm, CodeAssembler,
    },
};

pub fn encode(assembler: &mut CodeAssembler, amount: &Operand, value: &Operand) {
    match (amount, value) {
        (
            Operand {
                kind: I(amount), ..
            },
            Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_8,
            },
        ) => {
            if *amount >= 8 {
                assembler
                    .xor::<AsmRegister8, AsmRegister8>(
                        value.try_into().unwrap(),
                        value.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                assembler
                    .shl::<AsmRegister8, u32>(
                        value.try_into().unwrap(),
                        u32::try_from(*amount).unwrap(),
                    )
                    .unwrap();
            }
        }
        (
            Operand {
                kind: I(amount), ..
            },
            Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_16,
            },
        ) => {
            if *amount >= 16 {
                assembler
                    .xor::<AsmRegister16, AsmRegister16>(
                        value.try_into().unwrap(),
                        value.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                assembler
                    .shl::<AsmRegister16, u32>(
                        value.try_into().unwrap(),
                        u32::try_from(*amount).unwrap(),
                    )
                    .unwrap();
            }
        }
        (
            Operand {
                kind: I(amount), ..
            },
            Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_32,
            },
        ) => {
            if *amount >= 32 {
                assembler
                    .xor::<AsmRegister32, AsmRegister32>(
                        value.try_into().unwrap(),
                        value.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                assembler
                    .shl::<AsmRegister32, u32>(
                        value.try_into().unwrap(),
                        u32::try_from(*amount).unwrap(),
                    )
                    .unwrap();
            }
        }
        (
            Operand {
                kind: I(amount), ..
            },
            Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_64,
            },
        ) => {
            if *amount >= 64 {
                assembler
                    .xor::<AsmRegister64, AsmRegister64>(
                        value.try_into().unwrap(),
                        value.try_into().unwrap(),
                    )
                    .unwrap();
            } else {
                assembler
                    .shl::<AsmRegister64, u32>(
                        value.try_into().unwrap(),
                        u32::try_from(*amount).unwrap(),
                    )
                    .unwrap();
            }
        }
        (
            Operand {
                kind: I(amount), ..
            },
            Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_128,
            },
        ) => {
            assert!(amount % 8 == 0);
            assembler
                .pslldq::<AsmRegisterXmm, u32>(
                    value.try_into().unwrap(),
                    (amount / 8).try_into().unwrap(),
                )
                .unwrap();
        }
        (
            Operand {
                kind: R(PHYS(PhysicalRegister::RCX)),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_64,
            },
        ) => {
            assembler
                .shl::<AsmRegister64, AsmRegister8>(
                    value.try_into().unwrap(),
                    PhysicalRegister::RCX.try_into().unwrap(),
                )
                .unwrap();
        }
        (
            Operand {
                kind: R(PHYS(PhysicalRegister::RCX)),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(value)),
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .shl::<AsmRegister32, AsmRegister8>(
                    value.try_into().unwrap(),
                    PhysicalRegister::RCX.try_into().unwrap(),
                )
                .unwrap();
        }

        _ => todo!("shl {amount} {value}"),
    }
}
