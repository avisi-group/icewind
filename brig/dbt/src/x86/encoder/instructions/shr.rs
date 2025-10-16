use {
    crate::x86::encoder::{
        Operand,
        OperandKind::{Immediate as I, Register as R},
        Register::Physical as PHYS,
        Width,
        registers::PhysicalRegister,
    },
    iced_x86::code_asm::{
        AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, AsmRegisterXmm, CodeAssembler,
    },
 common::Alloc,
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, amount: &Operand<A>, value: &Operand<A>) {
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
            assembler
                .shr::<AsmRegister8, u32>(value.into(), u32::try_from(*amount).unwrap())
                .unwrap();
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
            assembler
                .shr::<AsmRegister16, u32>(value.into(), u32::try_from(*amount).unwrap())
                .unwrap();
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
            assembler
                .shr::<AsmRegister32, u32>(value.into(), u32::try_from(*amount).unwrap())
                .unwrap();
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
            assembler
                .shr::<AsmRegister64, u32>(value.into(), u32::try_from(*amount).unwrap())
                .unwrap();
        }
        (
            Operand {
                kind: R(PHYS(PhysicalRegister::RCX)),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(value_reg)),
                width_in_bits: Width::_64,
            },
        ) => {
            if *value_reg == PhysicalRegister::RCX {
                panic!("can't shr %rcx %rcx");
            }
            assembler
                .shr::<AsmRegister64, AsmRegister8>(value_reg.into(), PhysicalRegister::RCX.into())
                .unwrap();
        }
        (
            Operand {
                kind: R(PHYS(PhysicalRegister::RCX)),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(value_reg)),
                width_in_bits: Width::_32,
            },
        ) => {
            if *value_reg == PhysicalRegister::RCX {
                panic!("can't shr %rcx %rcx");
            }
            assembler
                .shr::<AsmRegister32, AsmRegister8>(value_reg.into(), PhysicalRegister::RCX.into())
                .unwrap();
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
            if amount % 8 != 0 {
                panic!("cannot shift xmm register right by {amount} bits")
            }

            assembler
                .psrldq::<AsmRegisterXmm, u32>(
                    value.try_into().unwrap(),
                    (amount / 8).try_into().unwrap(),
                )
                .unwrap();
        }
        _ => todo!("shr {amount} {value}"),
    }
}
