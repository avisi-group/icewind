use {
    crate::host::dbt::{
        Alloc as MemAlloc,
        x86::{
            encoder::{
                Instruction, Opcode, Operand, OperandKind, UseDef, UseDefMut,
                registers::{
                    PhysicalRegister, PhysicalRegisterGeneral, PhysicalRegisterXmm, Register,
                },
                width::Width,
            },
            register_allocator::{
                RegisterAllocator,
                naive::{physical_used::PhysicalUsed, range::Range},
            },
        },
    },
    alloc::vec::Vec,
    common::hashmap::{HashMap, HashSet},
    core::panic,
    strum::IntoEnumIterator,
};

mod physical_used;
mod range;

pub struct FreshAllocator {
    global_register_offset: usize,
}

impl FreshAllocator {
    pub fn new(_num_virt_regs: usize, global_register_offset: usize) -> Self {
        Self {
            global_register_offset,
        }
    }
}

impl RegisterAllocator for FreshAllocator {
    fn allocate<M: MemAlloc>(&mut self, instructions: &mut Vec<Instruction<M>, M>) {
        let live_ranges = build_live_ranges(instructions);

        let allocation_plan = build_allocation_plan(&live_ranges, instructions);

        //        log::warn!("plan: {allocation_plan:?}");

        // apply allocation plan
        instructions.iter_mut().for_each(|instruction| {
            instruction.get_operands_mut().for_each(|op| {
                if let Some((_, op)) = op {
                    if let OperandKind::Register(Register::Global(idx)) = op.kind() {
                        *op = Operand::mem_base_displ(
                            op.width(),
                            Register::Physical(PhysicalRegister::RBP),
                            i32::try_from(self.global_register_offset + (*idx * 8)).unwrap(),
                        )
                    }
                }
            });

            instruction.get_use_defs_mut().for_each(|ud| {
                let (UseDefMut::Def(reg) | UseDefMut::Use(reg) | UseDefMut::UseDef(reg)) = ud;
                if let Register::Virtual(vreg) = &*reg {
                    *reg = Register::Physical(*allocation_plan.get(vreg).unwrap());
                }
            });
        });

        // kill redundant mov's
        instructions.iter_mut().for_each(|instruction| {
            if let Opcode::MOV(src, dst) = instruction.0 {
                if src == dst {
                    instruction.0 = Opcode::DEAD;
                }
            }
        });

        insert_register_saves(&live_ranges, &allocation_plan, instructions);
    }
}

fn build_allocation_plan<M: MemAlloc>(
    live_ranges: &HashMap<Register, Vec<Range>>,
    instructions: &mut [Instruction<M>],
) -> HashMap<usize, PhysicalRegister> {
    let mut allocation_plan = HashMap::default();

    let mut physical_used = PhysicalUsed::empty();

    instructions
        .iter()
        .enumerate()
        .for_each(|(instruction_index, _instruction)| {
            build_at_instruction_index(
                live_ranges,
                &mut allocation_plan,
                &mut physical_used,
                instruction_index,
            );
        });

    allocation_plan
}

fn build_at_instruction_index(
    live_ranges: &HashMap<Register, Vec<Range>>,
    allocation_plan: &mut HashMap<usize, PhysicalRegister>,
    physical_used: &mut PhysicalUsed,
    instruction_index: usize,
) {
    // ended registers at this index
    live_ranges
        .iter()
        .map(|(reg, ranges)| ranges.iter().map(move |range| (*reg, *range)))
        .flatten()
        .filter(|(_, range)| range.end() == Some(instruction_index))
        .map(|(reg, _)| reg)
        .for_each(|reg| match reg {
            Register::Physical(phys_reg) => {
                physical_used.remove(&phys_reg);
            }
            Register::Virtual(idx) => {
                let phys_reg = allocation_plan.get(&idx).unwrap();
                physical_used.remove(phys_reg);
            }
            Register::Global(_) => {
                // TODO
            }
        });

    // registers that start at this index
    let started_registers = live_ranges
        .iter()
        .map(|(reg, ranges)| ranges.iter().map(move |range| (*reg, *range)))
        .flatten()
        .filter(|(_, range)| range.start() == instruction_index)
        .map(|(reg, range)| (reg, range.width()))
        .collect::<Vec<_>>();

    started_registers
        .iter()
        .filter_map(|(reg, width)| {
            if let Register::Physical(phys_reg) = reg {
                Some((phys_reg, width))
            } else {
                None
            }
        })
        .for_each(|(phys_reg, width)| {
            if physical_used.contains(phys_reg) {
                let currently_live_registers = live_ranges
                    .iter()
                    .filter(|(_, ranges)| {
                        ranges.iter().any(|range| range.contains(instruction_index))
                    })
                    .filter_map(|(reg, _)| {
                        if let Register::Virtual(idx) = reg {
                            Some(*idx)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<usize>>();

                // vregs that use our just-started physical register
                let mut vregs = allocation_plan
                    .iter()
                    .filter(|(vreg, preg)| {
                        *preg == phys_reg && currently_live_registers.contains(vreg)
                    })
                    .map(|(vreg, _)| *vreg)
                    .collect::<Vec<_>>();

                assert!(vregs.len() == 1);

                let conflicting_vreg = vregs.pop().unwrap();
                log::trace!(
                    "detected conflict with preg {phys_reg} and vreg {}",
                    conflicting_vreg
                );

                // todo: maybe only need to check intersections with start of current range

                // virt so should onyl have one range
                let vreg_range = live_ranges
                    .get(&Register::Virtual(conflicting_vreg))
                    .unwrap()[0];

                // now we need to choose a new phys reg

                // find all registers that intersect with the conflicting register
                let intersecting_registers = query_intersections(vreg_range, &live_ranges);

                let intersecting_physicals = intersecting_registers
                    .iter()
                    .filter_map(|reg| match reg {
                        // intersects in the future but not yet allocated
                        Register::Virtual(idx) => allocation_plan.get(&*idx).copied(),
                        Register::Physical(phys_reg) => Some(*phys_reg),
                        Register::Global(_) => None,
                    })
                    .collect::<Vec<_>>();

                // todo: maybe start at 0 and set bits, rather than copying currently used
                let mut temp_physical_used = physical_used.clone();
                for phys_reg in intersecting_physicals {
                    temp_physical_used.insert(phys_reg);
                }
                let reallocated_phys = allocate_physical_register(&temp_physical_used, *width);
                physical_used.insert(reallocated_phys);

                allocation_plan.insert(conflicting_vreg, reallocated_phys);
            } else {
                physical_used.insert(*phys_reg);
            }
        });

    started_registers
        .iter()
        .filter_map(|(reg, width)| if let Register::Virtual(idx) = reg { Some((idx, width)) } else { None })
        .for_each(|(vreg_idx, width)| {
            let phys_reg = allocate_physical_register(physical_used, *width);

            physical_used.insert(phys_reg);

            // assert that virtual register never re-starts
            if let Some(old_preg) = allocation_plan.insert(*vreg_idx, phys_reg) {
                panic!("cannot re-start virtual register! vreg: {vreg_idx}, old_preg: {old_preg}, new allocation: {phys_reg}");
            }
        })
}

fn allocate_physical_register(used: &PhysicalUsed, width: Width) -> PhysicalRegister {
    match width {
        Width::_128 => PhysicalRegisterXmm::iter()
            .map(PhysicalRegister::Xmm)
            .find(|phys_reg| !used.contains(phys_reg)),
        Width::_64 | Width::_32 | Width::_16 | Width::_8 => PhysicalRegisterGeneral::iter()
            .map(PhysicalRegister::General)
            .find(|phys_reg| !used.contains(phys_reg)),
    }
    .expect("failed to allocate physical register")
}

fn build_live_ranges<M: MemAlloc>(
    instructions: &mut [Instruction<M>],
) -> HashMap<Register, Vec<Range>> {
    let mut live_ranges = HashMap::default();

    // stores stack pointer from brig, can't clobber
    live_ranges.insert(
        Register::Physical(PhysicalRegister::RSP),
        alloc::vec![Range::new(0, usize::MAX, Width::_64)],
    );

    // register file pointer
    live_ranges.insert(
        Register::Physical(PhysicalRegister::RBP),
        alloc::vec![Range::new(0, usize::MAX, Width::_64)],
    );

    // debug register for panics
    live_ranges.insert(
        Register::Physical(PhysicalRegister::R15),
        alloc::vec![Range::new(0, usize::MAX, Width::_64)],
    );

    let instrs_clone = instructions.to_vec();
    log::debug!("before alloc ----------------------------");
    for (idx, i) in instrs_clone.iter().enumerate() {
        log::debug!("{idx}: {i}");
    }

    instructions
        .iter_mut()
        .enumerate()
        .for_each(|(instruction_index, instruction)| {
            if matches!(instruction.0, Opcode::RET) {
                if let Some(live_ranges) = live_ranges.get_mut(&Register::Physical(PhysicalRegister::RAX)) {
                    // update end
                    let last_range = live_ranges
                        .as_mut_slice()
                        .last_mut()
                        .expect("should have at least one live range")
                      ;

                    if last_range.end().unwrap_or_default() < instruction_index {
                        last_range.set_end(instruction_index);
                    }
                }
            } else {
                instruction
                    .get_use_defs()
                    .filter(|(ud, _)| {
                        !matches!(
                            ud,
                            UseDef::Def(Register::Global(_))
                                | UseDef::Use(Register::Global(_))
                                | UseDef::UseDef(Register::Global(_))
                        )
                    })
                    .for_each(|(ud, width)| {
                        let is_usedef = ud.is_usedef();
                        if let UseDef::Def(reg) | UseDef::UseDef(reg) = ud {
                            if is_usedef {
                                if let Opcode::XOR(l, r) = instruction.0 {
                                    if l == r {
                                        //
                                    } else {
                                        return;
                                    }
                                } else {
                                    return;
                                }
                            }

                            live_ranges
                                .entry(reg)
                                .and_modify(|live_ranges| {
                                    // assert last live range had some end
                                    let last_range = live_ranges.as_mut_slice().last_mut().unwrap();

                                    if last_range.end().is_none() {
                                        // silenced due to CMOVNE, will give it an end in a second
                                        // log::warn!(
                                        //     "last live range had no end, but re-def'd: {reg} in {}",
                                        //     instr_clone
                                        // );
                                        last_range.set_end(instruction_index);

                                    }

                                    // start new live range if past the current end
                                    if instruction_index >= last_range.end().unwrap_or_default() {
                                        if let Register::Virtual(_) = reg {
                                            if let Opcode::CMOVNE(_, _) = instruction.0 {
                                                // do nothing for CMOVNE
                                            } else {
                                                panic!(
                                                    "cannot re-start virtual register {reg} in instr {}",
                                                    instruction
                                                )
                                            }
                                        } else {
                                            live_ranges.push(Range::new_partial(instruction_index, width));
                                        }
                                    }
                                })
                                .or_insert(alloc::vec![Range::new_partial(instruction_index, width)]);
                        }
                    });

                instruction
                    .get_use_defs()
                    .filter(|(ud, _)| {
                        !matches!(
                            ud,
                            UseDef::Def(Register::Global(_))
                                | UseDef::Use(Register::Global(_))
                                | UseDef::UseDef(Register::Global(_))
                        )
                    })
                    .for_each(|(ud, width)| {
                        if let UseDef::Use(reg) | UseDef::UseDef(reg) = ud {
                            // assert exists
                            let live_ranges = live_ranges
                                .get_mut(&reg)
                                .unwrap_or_else(|| panic!("use of undef'd register {reg} @ {instruction_index}"));

                            // update end
                            let last_range = live_ranges
                                .as_mut_slice()
                                .last_mut()
                                .expect("should have at least one live range");

                                // update width
                            last_range.set_width(core::cmp::max(width, last_range.width()));

                            if last_range.end().unwrap_or_default() < instruction_index {
                                last_range.set_end(instruction_index);

                            }
                        }
                    });
            }
        });

    live_ranges
}

fn insert_register_saves<M: MemAlloc>(
    live_ranges: &HashMap<Register, Vec<Range>>,
    allocation_plan: &HashMap<usize, PhysicalRegister>,
    instructions: &mut Vec<Instruction<M>, M>,
) {
    const CALLER_SAVED: &[PhysicalRegister] = &[
        PhysicalRegister::RAX,
        PhysicalRegister::RCX,
        PhysicalRegister::RDX,
        PhysicalRegister::RSI,
        PhysicalRegister::RDI,
        PhysicalRegister::R8,
        PhysicalRegister::R9,
        PhysicalRegister::R10,
        PhysicalRegister::R11,
    ];

    let mut new_instructions = alloc::vec![];

    for (index, instr) in instructions.iter().enumerate() {
        if let Instruction(Opcode::CALL { .. }) = instr {
            let live_registers = live_ranges // only caller_saved live ranges
                .iter()
                .filter(|(_, ranges)| {
                    ranges
                        .iter()
                        .copied()
                        .filter(|r| !r.is_partial())
                        .filter_map(|r| r.end().map(|end| (r.start(), end)))
                        .any(|(start, end)| start < index && end > index)
                })
                .map(|(reg, _)| match reg {
                    Register::Physical(preg) => *preg,
                    Register::Virtual(virt) => *allocation_plan.get(virt).unwrap(),
                    Register::Global(_) => todo!(),
                })
                .collect::<Vec<_>>();

            let to_save = CALLER_SAVED
                .iter()
                .filter(|r| live_registers.contains(r))
                .collect::<Vec<_>>();

            for reg in to_save.iter() {
                new_instructions.push(Instruction::push(Operand::preg(Width::_64, **reg)))
            }

            new_instructions.push(*instr);

            for reg in to_save.iter().rev() {
                new_instructions.push(Instruction::pop(Operand::preg(Width::_64, **reg)))
            }
        } else {
            new_instructions.push(*instr);
        }
    }

    instructions.clear();
    instructions.extend_from_slice(&new_instructions);
}

fn query_intersections(x: Range, live_ranges: &HashMap<Register, Vec<Range>>) -> HashSet<Register> {
    live_ranges
        .iter()
        .map(|(reg, ranges)| ranges.iter().map(move |range| (*reg, *range)))
        .flatten()
        .filter(|(_, y)| x.intersects(y))
        .map(|(reg, _)| reg)
        .collect()
}
