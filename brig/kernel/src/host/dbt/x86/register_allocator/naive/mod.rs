use {
    crate::host::dbt::{
        Alloc as MemAlloc,
        x86::{
            encoder::{
                Instruction, Opcode, Operand, OperandKind, UseDef, UseDefMut,
                registers::{PhysicalRegister, PhysicalRegisterGeneral, Register},
                width::Width,
            },
            register_allocator::{RegisterAllocator, naive::physical_used::PhysicalUsed},
        },
    },
    alloc::vec::Vec,
    common::hashmap::{HashMap, HashSet},
    core::panic,
    strum::IntoEnumIterator,
};

mod physical_used;

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

        // apply allocation plan
        instructions.iter_mut().for_each(|instruction| {
            instruction.get_operands_mut().for_each(|op| {
                if let Some((_, op)) = op {
                    if let Operand {
                        kind: OperandKind::Register(Register::Global(idx)),
                        width_in_bits,
                    } = op
                    {
                        *op = Operand::mem_base_displ(
                            *width_in_bits,
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
    live_ranges: &HashMap<Register, Vec<(usize, Option<usize>)>>,
    instructions: &mut [Instruction<M>],
) -> HashMap<usize, PhysicalRegister> {
    let mut allocation_plan = HashMap::default();

    let mut physical_used = PhysicalUsed::empty();

    instructions.iter().enumerate().for_each(|(instruction_index, _instruction)| {
        {
            let ended_registers = live_ranges
                .iter()
                .map(|(reg, ranges)| ranges.iter().map(move |range| (*reg, *range)))
                .flatten()
                .filter(|(_, (_, end))| *end == Some(instruction_index))
                .map(|(reg, _)| reg)
                .collect::<Vec<_>>();

            ended_registers.iter().for_each(|reg| match reg {
                Register::Physical(phys_reg) => {
                  physical_used.remove(phys_reg);
                }
                Register::Virtual(idx) => {
                    let phys_reg = allocation_plan.get(&*idx).unwrap();
                    physical_used.remove(phys_reg);
                }
                Register::Global(_) => {
                    // TODO
                }
            });
        }

        let started_registers = live_ranges
            .iter()
            .map(|(reg, ranges)| ranges.iter().map(move |range| (*reg, *range)))
            .flatten()
            .filter(|(_, (start, _))| *start == instruction_index)
            .map(|(reg, _)| reg)
            .collect::<Vec<_>>();

        started_registers
            .iter()
            .filter_map(|reg| if let Register::Physical(phys_reg) = reg { Some(phys_reg) } else { None })
            .for_each(|phys_reg| {
                if physical_used.contains(phys_reg) {
                    let currently_live_registers = live_ranges
                        .iter()
                        .filter(|(_, ranges)|
                            ranges.iter().any(|(start, end)|
                            (*start <= instruction_index) &&
                            (instruction_index < end.unwrap()
                        )))
                        .filter_map(|(reg, _)| if let Register::Virtual(idx) = reg { Some(*idx) } else { None })
                        .collect::<Vec<usize>>();

                    // vregs that use our just-started physical register
                    let mut vregs = allocation_plan
                        .iter()
                        .filter(|(vreg, preg)| *preg == phys_reg && currently_live_registers.contains(vreg))
                        .map(|(vreg, _)| *vreg)
                        .collect::<Vec<_>>();

                    assert!(vregs.len() == 1);

                    let conflicting_vreg = vregs.pop().unwrap();
                    log::trace!("detected conflict with preg {phys_reg} and vreg {}", conflicting_vreg);

                    // todo: maybe only need to check intersections with start of current range

                    // virt so should onyl have one range
                    let vreg_range = live_ranges.get(&Register::Virtual(conflicting_vreg)).unwrap()[0];

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
                    let reallocated_phys = PhysicalRegisterGeneral::iter().map(PhysicalRegister::General).find(|phys_reg| !temp_physical_used.contains(phys_reg)).unwrap();
                    physical_used.insert(reallocated_phys);

                    allocation_plan.insert(conflicting_vreg, reallocated_phys);
                } else {
                    physical_used.insert(*phys_reg);
                }
            });

        started_registers
            .iter()
            .filter_map(|reg| if let Register::Virtual(idx) = reg { Some(idx) } else { None })
            .for_each(|vreg_idx| {
                let phys_reg = PhysicalRegisterGeneral::iter().map(PhysicalRegister::General).find(|phys_reg| !physical_used.contains(phys_reg)).unwrap();

                physical_used.insert(phys_reg);

                // assert that virtual register never re-starts
                if let Some(old_preg) = allocation_plan.insert(*vreg_idx, phys_reg) {
                    panic!("cannot re-start virtual register! vreg: {vreg_idx}, old_preg: {old_preg}, new allocation: {phys_reg}");
                }
            })
    });

    allocation_plan
}

fn build_live_ranges<M: MemAlloc>(
    instructions: &mut [Instruction<M>],
) -> HashMap<Register, Vec<(usize, Option<usize>)>> {
    let mut live_ranges = HashMap::default();

    // stores stack pointer from brig, can't clobber
    live_ranges.insert(
        Register::Physical(PhysicalRegister::RSP),
        alloc::vec![(0, Some(usize::MAX))],
    );

    // register file pointer
    live_ranges.insert(
        Register::Physical(PhysicalRegister::RBP),
        alloc::vec![(0, Some(usize::MAX))],
    );

    // debug register for panics
    live_ranges.insert(
        Register::Physical(PhysicalRegister::R15),
        alloc::vec![(0, Some(usize::MAX))],
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
                    let last_use = &mut live_ranges
                        .as_mut_slice()
                        .last_mut()
                        .expect("should have at least one live range")
                        .1;

                    if last_use.unwrap_or_default() < instruction_index {
                        *last_use = Some(instruction_index);
                    }
                }
            } else {
                instruction
                    .get_use_defs()
                    .filter(|ud| {
                        !matches!(
                            ud,
                            UseDef::Def(Register::Global(_))
                                | UseDef::Use(Register::Global(_))
                                | UseDef::UseDef(Register::Global(_))
                        )
                    })
                    .for_each(|ud| {
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

                                    if last_range.1.is_none() {
                                        // silenced due to CMOVNE, will give it an end in a second
                                        // log::warn!(
                                        //     "last live range had no end, but re-def'd: {reg} in {}",
                                        //     instr_clone
                                        // );
                                        last_range.1 = Some(instruction_index);
                                    }

                                    // start new live range if past the current end
                                    if instruction_index >= last_range.1.unwrap_or_default() {
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
                                            live_ranges.push((instruction_index, None));
                                        }
                                    }
                                })
                                .or_insert(alloc::vec![(instruction_index, None)]);
                        }
                    });
                instruction
                    .get_use_defs()
                    .filter(|ud| {
                        !matches!(
                            ud,
                            UseDef::Def(Register::Global(_))
                                | UseDef::Use(Register::Global(_))
                                | UseDef::UseDef(Register::Global(_))
                        )
                    })
                    .for_each(|ud| {
                        if let UseDef::Use(reg) | UseDef::UseDef(reg) = ud {
                            // assert exists
                            let live_ranges = live_ranges
                                .get_mut(&reg)
                                .unwrap_or_else(|| panic!("use of undef'd register {reg} @ {instruction_index}"));

                            // update end
                            let last_use = &mut live_ranges
                                .as_mut_slice()
                                .last_mut()
                                .expect("should have at least one live range")
                                .1;

                            if last_use.unwrap_or_default() < instruction_index {
                                *last_use = Some(instruction_index);
                            }
                        }
                    });
            }
        });

    live_ranges
}

fn insert_register_saves<M: MemAlloc>(
    live_ranges: &HashMap<Register, Vec<(usize, Option<usize>)>>,
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
                        .filter_map(|(start, end)| end.map(|end| (start, end)))
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

fn query_intersections(
    (x_start, x_end): (usize, Option<usize>),
    live_ranges: &HashMap<Register, Vec<(usize, Option<usize>)>>,
) -> HashSet<Register> {
    live_ranges
        .iter()
        .map(|(reg, ranges)| ranges.iter().map(move |range| (*reg, *range)))
        .flatten()
        .filter(|(_, (y_start, y_end))| x_start <= y_end.unwrap() && *y_start <= x_end.unwrap())
        .map(|(reg, _)| reg)
        .collect()
}
