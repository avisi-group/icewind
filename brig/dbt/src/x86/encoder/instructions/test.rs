use {
    crate::x86::encoder::{
        Operand, OperandKind::Register as R, Width, registers::Register::Physical as PHYS,
    },
    iced_x86::code_asm::{AsmRegister8, AsmRegister32, AsmRegister64, CodeAssembler},
    shared::Alloc,
};

pub fn encode<A: Alloc>(assembler: &mut CodeAssembler, src: &Operand<A>, dst: &Operand<A>) {
    match (src, dst) {
        // TEST R, R
        (
            Operand {
                kind: R(PHYS(left)),
                width_in_bits: Width::_64,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_64,
            },
        ) => {
            assembler
                .test::<AsmRegister64, AsmRegister64>(left.into(), right.into())
                .unwrap();
        }

        // TEST R, R
        (
            Operand {
                kind: R(PHYS(left)),
                width_in_bits: Width::_8,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_8,
            },
        ) => {
            assembler
                .test::<AsmRegister8, AsmRegister8>(left.into(), right.into())
                .unwrap();
        }
        (
            Operand {
                kind: R(PHYS(left)),
                width_in_bits: Width::_32,
            },
            Operand {
                kind: R(PHYS(right)),
                width_in_bits: Width::_32,
            },
        ) => {
            assembler
                .test::<AsmRegister32, AsmRegister32>(left.into(), right.into())
                .unwrap();
        }
        _ => todo!("test {src} {dst}"),
    }
}
