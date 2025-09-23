use {
    crate::host::dbt::{
        Alloc as MemAlloc,
        x86::{
            encoder::{
                Instruction, Opcode, Operand, registers::PhysicalRegister, registers::Register,
                width::Width,
            },
            register_allocator::naive::FreshAllocator, //solid_state::SolidStateRegisterAllocator,
        },
    },
    alloc::{alloc::Global, vec::Vec},
    proc_macro_lib::ktest,
};

//pub mod reverse_scan;
pub mod naive;
//pub mod solid_state;
pub mod regalloc_ng;

pub trait RegisterAllocator {
    // A is for the generic memory allocator, NOT anything to do with the register
    // allocator
    fn allocate<A: MemAlloc>(&mut self, instructions: &mut Vec<Instruction<A>, A>);
}

#[ktest]
fn conflicted_physical_allocation() {
    let mut instrs = alloc::vec![
        Instruction(Opcode::MOV(
            Operand::<Global>::imm(Width::_64, 0xaaaa),
            Operand::vreg(Width::_64, 0),
        )),
        Instruction(Opcode::MOV(
            Operand::imm(Width::_64, 0xaaaa),
            Operand::preg(Width::_64, PhysicalRegister::RAX),
        )),
        Instruction(Opcode::MOV(
            Operand::preg(Width::_64, PhysicalRegister::RAX),
            Operand::mem_base(Width::_64, Register::Virtual(0)),
        )),
        Instruction(Opcode::MOV(
            Operand::vreg(Width::_64, 0),
            Operand::mem_base(Width::_64, Register::Virtual(0)),
        )),
    ];

    let mut allocator = FreshAllocator::new(2, 0);
    allocator.allocate(&mut instrs);
}

// #[ktest]
// fn shr_full() {
//     use {
//         crate::host::dbt::x86::{
//             OperandKind::*,
//             encoder::registers::{PhysicalRegister, Register::*},
//         },
//         Opcode::*,
//     };

//     let mut instructions = alloc::vec![
//         Instruction(MOV(
//             Operand {
//                 kind: Memory {
//                     base: Some(Physical(PhysicalRegister::RBP)),
//                     index: None,
//                     scale: MemoryScale::S1,
//                     displacement: 12560,
//                     segment_override: None,
//                 },
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(4)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(4)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(5)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(SHR(
//             Operand {
//                 kind: Immediate(0),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(5)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(5)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(3)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Immediate(18446744073709551615),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(6)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(3)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(2)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(AND(
//             Operand {
//                 kind: Register(Virtual(6)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(2)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(2)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(1)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Immediate(64),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(7)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Immediate(0),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Physical(PhysicalRegister::RDX)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(1)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Physical(PhysicalRegister::RAX)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(IDIV(
//             Operand {
//                 kind: Register(Physical(PhysicalRegister::RDX)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Physical(PhysicalRegister::RAX)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(7)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Memory {
//                     base: Some(Physical(PhysicalRegister::RBP)),
//                     index: None,
//                     scale: MemoryScale::S1,
//                     displacement: 4984,
//                     segment_override: None,
//                 },
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(10)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(10)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(11)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(SHR(
//             Operand {
//                 kind: Immediate(0),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(11)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(11)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(9)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Immediate(18446744073709551615),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(12)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(9)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(8)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(AND(
//             Operand {
//                 kind: Register(Virtual(12)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(8)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(8)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(13)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Physical(PhysicalRegister::RDX)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Physical(PhysicalRegister::RCX)),
//                 width_in_bits: Width::_8,
//             },
//         )),
//         Instruction(SHR(
//             Operand {
//                 kind: Register(Physical(PhysicalRegister::RCX)),
//                 width_in_bits: Width::_8,
//             },
//             Operand {
//                 kind: Register(Virtual(13)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(13)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Register(Virtual(0)),
//                 width_in_bits: Width::_64,
//             },
//         )),
//         Instruction(MOV(
//             Operand {
//                 kind: Register(Virtual(0)),
//                 width_in_bits: Width::_64,
//             },
//             Operand {
//                 kind: Memory {
//                     base: Some(Physical(PhysicalRegister::RBP)),
//                     index: None,
//                     scale: MemoryScale::S1,
//                     displacement: 12560,
//                     segment_override: None,
//                 },
//                 width_in_bits: Width::_64,
//             },
//         )),
//     ];

//     let mut allocator = FreshAllocator::new(14, 0);
//     allocator.allocate(&mut instructions);
// }

/*Instruction {
        opcode: Opcode::MOV,
        operands: alloc::vec![
            (
                OperandDirection::In,
                Operand {
                    kind: OperandKind::Memory {
                        base: Some(Register::PhysicalRegister(PhysicalRegister::RBP)),
                        index: None,
                        scale: MemoryScale::S1,
                        displacement: 0x1234,
                        segment_override: None,
                    },
                    width_in_bits: 0x40,
                },
            ),
            (
                OperandDirection::Out,
                Operand {
                    kind: OperandKind::Register(Register::VirtualRegister(0x1)),
                    width_in_bits: 0x40,
                },
            ),
        ],
    },
    Instruction {
        opcode: Opcode::MOV,
        operands: alloc::vec![
            (
                OperandDirection::In,
                Operand {
                    kind: OperandKind::Register(Register::VirtualRegister(0x1)),
                    width_in_bits: 0x40,
                },
            ),
            (
                OperandDirection::Out,
                Operand {
                    kind: OperandKind::Register(Register::VirtualRegister(0x0)),
                    width_in_bits: 0x40,
                },
            ),
        ],
    },
    Instruction {
        opcode: Opcode::ADD,
        operands: alloc::vec![
            (
                OperandDirection::In,
                Operand {
                    kind: OperandKind::Immediate(0x1),
                    width_in_bits: 0x20,
                },
            ),
            (
                OperandDirection::InOut,
                Operand {
                    kind: OperandKind::Register(Register::VirtualRegister(0x0)),
                    width_in_bits: 0x40,
                },
            ),
        ],
    },
    Instruction {
        opcode: Opcode::MOV,
        operands: alloc::vec![
            (
                OperandDirection::In,
                Operand {
                    kind: OperandKind::Register(Register::VirtualRegister(0x0)),
                    width_in_bits: 0x40,
                },
            ),
            (
                OperandDirection::Out,
                Operand {
                    kind: OperandKind::Memory {
                        base: Some(Register::PhysicalRegister(PhysicalRegister::RBP)),
                        index: None,
                        scale: MemoryScale::S1,
                        displacement: 0x1234,
                        segment_override: None,
                    },
                    width_in_bits: 0x40,
                },
            ),
        ],
    },
];*/

/*assert_eq!(
    instrs,
    [
        Instruction {
            opcode: Opcode::MOV,
            operands: alloc::vec![
                (
                    OperandDirection::In,
                    Operand {
                        kind: OperandKind::Memory {
                            base: Some(Register::PhysicalRegister(PhysicalRegister::RBP)),
                            index: None,
                            scale: MemoryScale::S1,
                            displacement: 0x1234,
                            segment_override: None,
                        },
                        width_in_bits: 0x40,
                    },
                ),
                (
                    OperandDirection::Out,
                    Operand {
                        kind: OperandKind::Register(Register::PhysicalRegister(
                            PhysicalRegister::RCX
                        )),
                        width_in_bits: 0x40,
                    },
                ),
            ],
        },
        Instruction {
            opcode: Opcode::MOV,
            operands: alloc::vec![
                (
                    OperandDirection::In,
                    Operand {
                        kind: OperandKind::Register(Register::PhysicalRegister(
                            PhysicalRegister::RCX
                        )),
                        width_in_bits: 0x40,
                    },
                ),
                (
                    OperandDirection::Out,
                    Operand {
                        kind: OperandKind::Register(Register::PhysicalRegister(
                            PhysicalRegister::RAX
                        )),
                        width_in_bits: 0x40,
                    },
                ),
            ],
        },
        Instruction {
            opcode: Opcode::ADD,
            operands: alloc::vec![
                (
                    OperandDirection::In,
                    Operand {
                        kind: OperandKind::Immediate(0x1),
                        width_in_bits: 0x20,
                    },
                ),
                (
                    OperandDirection::InOut,
                    Operand {
                        kind: OperandKind::Register(Register::PhysicalRegister(
                            PhysicalRegister::RAX
                        )),
                        width_in_bits: 0x40,
                    },
                ),
            ],
        },
        Instruction {
            opcode: Opcode::MOV,
            operands: alloc::vec![
                (
                    OperandDirection::In,
                    Operand {
                        kind: OperandKind::Register(Register::PhysicalRegister(
                            PhysicalRegister::RAX
                        )),
                        width_in_bits: 0x40,
                    },
                ),
                (
                    OperandDirection::Out,
                    Operand {
                        kind: OperandKind::Memory {
                            base: Some(Register::PhysicalRegister(PhysicalRegister::RBP)),
                            index: None,
                            scale: MemoryScale::S1,
                            displacement: 0x1234,
                            segment_override: None,
                        },
                        width_in_bits: 0x40,
                    },
                ),
            ],
        },
    ]
);*/
