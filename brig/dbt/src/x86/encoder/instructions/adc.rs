use {
    crate::x86::encoder::{
        Operand,
        OperandKind::{Immediate as I, Register as R},
        Width,
        registers::Register::Physical as PHYS,
    },
    brig_common::Alloc,
    iced_x86::code_asm::{AsmRegister8, AsmRegister32, AsmRegister64, CodeAssembler},
};

pub fn encode<A: Alloc>(
    assembler: &mut CodeAssembler,
    src: &Operand<A>,
    dst: &Operand<A>,
    carry: &Operand<A>,
) {
    match (src, dst, carry) {
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(carry)),
                width_in_bits: Width::_8,
            },
        ) => {
            // sets the carry flag
            assembler
                .add::<AsmRegister8, _>(carry.into(), 0xffff_ffffu32 as i32)
                .unwrap();

            assembler
                .adc::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
                .unwrap();
        }

        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(carry)),
                width_in_bits: Width::_8,
            },
        ) => {
            // sets the carry flag
            assembler
                .add::<AsmRegister8, _>(carry.into(), 0xffff_ffffu32 as i32)
                .unwrap();

            assembler
                .adc::<AsmRegister32, AsmRegister32>(dst.into(), src.into())
                .unwrap();
        }

        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: I(carry_in),
                width_in_bits: Width::_8,
            },
        ) => match carry_in {
            0 => {
                assembler
                    .add::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
                    .unwrap();
            }
            1 => {
                assembler.stc().unwrap();
                assembler
                    .adc::<AsmRegister64, AsmRegister64>(dst.into(), src.into())
                    .unwrap();
            }
            _ => panic!(),
        },
        (
            Operand {
                kind: R(PHYS(src)),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: I(carry_in),
                width_in_bits: Width::_8,
            },
        ) => match carry_in {
            0 => {
                assembler
                    .add::<AsmRegister32, AsmRegister32>(dst.into(), src.into())
                    .unwrap();
            }
            1 => {
                assembler.stc().unwrap();
                assembler
                    .adc::<AsmRegister32, AsmRegister32>(dst.into(), src.into())
                    .unwrap();
            }
            _ => panic!(),
        },
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: I(carry_in),
                width_in_bits: Width::_8,
            },
        ) => {
            let src = src.wrapping_add(*carry_in);

            assembler
                .add::<AsmRegister64, i32>(dst.into(), src.try_into().unwrap())
                .unwrap();
        }
        (
            Operand {
                kind: I(src),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: I(carry_in),
                width_in_bits: Width::_8,
            },
        ) => {
            let src = src.wrapping_add(*carry_in);

            assembler
                .add::<AsmRegister32, i32>(dst.into(), src.try_into().unwrap())
                .unwrap();
        }
        (
            Operand {
                kind: I(0),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(carry)),
                width_in_bits: Width::_8,
            },
        ) => {
            assembler
                .movzx::<AsmRegister64, AsmRegister8>(carry.into(), carry.into())
                .unwrap();
            assembler
                .add::<AsmRegister64, AsmRegister64>(dst.into(), carry.into())
                .unwrap();
        }
        (
            Operand {
                kind: I(0),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(dst)),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(carry)),
                width_in_bits: Width::_8,
            },
        ) => {
            assembler
                .movzx::<AsmRegister32, AsmRegister8>(carry.into(), carry.into())
                .unwrap();
            assembler
                .add::<AsmRegister32, AsmRegister32>(dst.into(), carry.into())
                .unwrap();
        }
        _ => todo!("adc {src} {dst} {carry}"),
    }
}
