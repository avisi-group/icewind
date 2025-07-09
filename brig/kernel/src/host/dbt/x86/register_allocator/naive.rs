use {
    crate::host::dbt::{
        Alloc as MemAlloc,
        x86::{
            encoder::{
                Instruction, Opcode, Operand, OperandKind, PhysicalRegister, Register, UseDef,
                UseDefMut, width::Width,
            },
            register_allocator::RegisterAllocator,
        },
    },
    alloc::vec::Vec,
    common::hashmap::{HashMap, HashSet},
    core::panic,
    strum::IntoEnumIterator,
};

pub struct FreshAllocator {
    global_register_offset: usize,
    live_ranges: HashMap<Register, Vec<(usize, Option<usize>)>>,
    // virtual register id to physical register
    allocation_plan: HashMap<usize, PhysicalRegister>,
}

impl RegisterAllocator for FreshAllocator {
    fn allocate<M: MemAlloc>(&mut self, instructions: &mut Vec<Instruction<M>, M>) {
        log::debug!("----------------------");

        self.build_live_ranges(instructions);

        #[cfg(feature = "debug-reg-alloc")]
        self.live_ranges.iter().for_each(|(reg, ranges)| {
            log::debug!(
                "{reg:?} = {}",
                ranges
                    .iter()
                    .map(|(start, end)| alloc::format!("{start} -> {end:?}"))
                    .join(", ")
            )
        });

        self.build_allocation_plan(instructions);

        #[cfg(feature = "debug-reg-alloc")]
        self.allocation_plan
            .iter()
            .for_each(|(vreg, preg)| log::debug!("{vreg} = {preg}",));

        // apply allocation plan
        instructions.iter_mut().for_each(|instruction| {
            instruction.get_operands_mut().for_each(|op| {
                if let Some((_, op)) = op {
                    if let Operand {
                        kind: OperandKind::Register(Register::GlobalRegister(idx)),
                        width_in_bits,
                    } = op
                    {
                        *op = Operand::mem_base_displ(
                            *width_in_bits,
                            Register::PhysicalRegister(PhysicalRegister::RBP),
                            i32::try_from(self.global_register_offset + (*idx * 8)).unwrap(),
                        )
                    }
                }
            });

            instruction.get_use_defs_mut().for_each(|ud| {
                let (UseDefMut::Def(reg) | UseDefMut::Use(reg) | UseDefMut::UseDef(reg)) = ud;
                if let Register::VirtualRegister(vreg) = &*reg {
                    *reg = Register::PhysicalRegister(*self.allocation_plan.get(vreg).unwrap());
                }
            });
        });

        instructions.iter_mut().for_each(|instruction| {
            if let Opcode::MOV(src, dst) = instruction.0 {
                if src == dst {
                    instruction.0 = Opcode::DEAD;
                }
            }
        });

        self.insert_register_saves(instructions);

        log::debug!("post alloc----------------------------");
        for i in instructions {
            log::debug!("{i}");
        }
    }
}

impl FreshAllocator {
    pub fn new(_num_virt_regs: usize, global_register_offset: usize) -> Self {
        Self {
            live_ranges: HashMap::default(),
            allocation_plan: HashMap::default(),
            global_register_offset,
        }
    }

    fn build_live_ranges<M: MemAlloc>(&mut self, instructions: &mut [Instruction<M>]) {
        // stores stack pointer from brig, can't clobber
        self.live_ranges.insert(
            Register::PhysicalRegister(PhysicalRegister::RSP),
            alloc::vec![(0, Some(usize::MAX))],
        );

        // register file pointer
        self.live_ranges.insert(
            Register::PhysicalRegister(PhysicalRegister::RBP),
            alloc::vec![(0, Some(usize::MAX))],
        );

        // debug register for panics
        self.live_ranges.insert(
            Register::PhysicalRegister(PhysicalRegister::R15),
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
                let instr_clone = instruction.clone();

                if matches!(instruction.0, Opcode::RET) {
                    if let Some(live_ranges) = self
                        .live_ranges
                        .get_mut(&Register::PhysicalRegister(PhysicalRegister::RAX))
                    {
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
                                UseDef::Def(Register::GlobalRegister(_))
                                    | UseDef::Use(Register::GlobalRegister(_))
                                    | UseDef::UseDef(Register::GlobalRegister(_))
                            )
                        })
                        .for_each(|ud| {
                            let is_usedef = ud.is_usedef();
                            if let UseDef::Def(reg) | UseDef::UseDef(reg) = ud {
                                if is_usedef {
                                    if let Opcode::XOR(l, r) = instr_clone.0 {
                                        if l == r {
                                            //
                                        } else {
                                            return;
                                        }
                                    } else {
                                        return;
                                    }
                                }

                                self.live_ranges
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
                                    if let Register::VirtualRegister(_) = reg {
                                        if let Opcode::CMOVNE(_, _) = instr_clone.0 {
                                            // do nothing for CMOVNE
                                        } else {
                                            panic!(
                                            "cannot re-start virtual register {reg} in instr {}",
                                            instr_clone
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
                                UseDef::Def(Register::GlobalRegister(_))
                                    | UseDef::Use(Register::GlobalRegister(_))
                                    | UseDef::UseDef(Register::GlobalRegister(_))
                            )
                        })
                        .for_each(|ud| {
                            if let UseDef::Use(reg) | UseDef::UseDef(reg) = ud {
                                // assert exists
                                let live_ranges =
                                    self.live_ranges.get_mut(&reg).unwrap_or_else(|| {
                                        panic!(
                                            "use of undef'd register {reg} @ {instruction_index}"
                                        )
                                    });

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
    }

    fn build_allocation_plan<M: MemAlloc>(&mut self, instructions: &mut [Instruction<M>]) {
        let mut physical_used = HashSet::default();

        instructions.iter().enumerate().for_each(|(instruction_index, _instruction)| {
            {
                let ended_registers = self.live_ranges.iter().map(|(reg, ranges)| ranges.iter().map(move |range| (*reg, *range))).flatten().filter(|(_, (_, end))| *end == Some(instruction_index)).map(|(reg, _)| reg).collect::<Vec<_>>();

                ended_registers.iter().for_each(|reg| match reg {
                    Register::PhysicalRegister(phys_reg) => {
                        assert!(physical_used.contains(phys_reg));
                        physical_used.remove(phys_reg);
                    }
                    Register::VirtualRegister(idx) => {
                        let phys_reg = self.allocation_plan.get(&*idx).unwrap();
                      assert!(physical_used.contains(phys_reg));
                        physical_used.remove(phys_reg);
                    }
                    Register::GlobalRegister(_) => {
                        // TODO
                    }
                });
            }

            let started_registers = self.live_ranges.iter().map(|(reg, ranges)| ranges.iter().map(move |range| (*reg, *range))).flatten().filter(|(_, (start, _))| *start == instruction_index).map(|(reg, _)| reg).collect::<Vec<_>>();

            started_registers.iter().filter_map(|reg| if let Register::PhysicalRegister(phys_reg) = reg { Some(phys_reg) } else { None }).for_each(|phys_reg| {
                if physical_used.contains(phys_reg) {
                    let currently_live_registers = self.live_ranges.iter().filter(|(_, ranges)| ranges.iter().any(|(start, end)| (*start <= instruction_index) && (instruction_index < end.unwrap()))).filter_map(|(reg, _)| if let Register::VirtualRegister(idx) = reg { Some(*idx) } else { None }).collect::<Vec<usize>>();

                    // vregs that use our just-started physical register
                    let mut vregs = self.allocation_plan.iter().filter(|(vreg, preg)| *preg == phys_reg && currently_live_registers.contains(vreg)).map(|(vreg, _)| *vreg).collect::<Vec<_>>();

                    assert!(vregs.len() == 1);

                    let conflicting_vreg = vregs.pop().unwrap();
                    log::trace!("detected conflict with preg {phys_reg} and vreg {}", conflicting_vreg);

                    // todo: maybe only need to check intersections with start of current range

                    // virt so should onyl have one range
                    let vreg_range = self.live_ranges.get(&Register::VirtualRegister(conflicting_vreg)).unwrap()[0];


                    // now we need to choose a new phys reg

                    // find all registers that intersect with the conflicting register
                let intersecting_registers=    query_intersections(vreg_range, &self.live_ranges);

               let intersecting_physicals = intersecting_registers.iter().filter_map(|reg| match reg {
                    Register::VirtualRegister(idx) => self.allocation_plan.get(&*idx).copied(), // intersects in the future but not yet allocated
                    Register::PhysicalRegister(phys_reg) => Some(*phys_reg),
                    Register::GlobalRegister(_) => None
                }).collect::<Vec<_>>();

                // todo: maybe start at 0 and set bits, rather than copying currently used
                    let mut temp_physical_used = physical_used.clone();
                    for phys_reg in intersecting_physicals {
                        temp_physical_used.insert(phys_reg);
                    }
                    let reallocated_phys =
                         PhysicalRegister::iter().find(|phys_reg| !temp_physical_used.contains(phys_reg)).unwrap();
                    physical_used.insert(reallocated_phys);

                    self.allocation_plan.insert(conflicting_vreg, reallocated_phys);
                } else {
                    physical_used.insert(*phys_reg);
                }
            });

            started_registers.iter().filter_map(|reg| if let Register::VirtualRegister(idx) = reg { Some(idx) } else { None }).for_each(|vreg_idx| {
                let phys_reg =  PhysicalRegister::iter().find(|phys_reg| !physical_used.contains(phys_reg)).unwrap();

                physical_used.insert(phys_reg);

                // assert that virtual register never re-starts
                if let Some(old_preg) = self.allocation_plan.insert(*vreg_idx, phys_reg) {
                    panic!("cannot re-start virtual register! vreg = {vreg_idx}, old_preg = {old_preg}, new allocation = {phys_reg}");
                }
            })
        });
    }

    fn insert_register_saves<M: MemAlloc>(&self, instructions: &mut Vec<Instruction<M>, M>) {
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
                let live_registers = self
                    .live_ranges // only caller_saved live ranges
                    .iter()
                    .filter(|(_, ranges)| {
                        ranges
                            .iter()
                            .copied()
                            .filter_map(|(start, end)| end.map(|end| (start, end)))
                            .any(|(start, end)| start < index && end > index)
                    })
                    .map(|(reg, _)| match reg {
                        Register::PhysicalRegister(preg) => *preg,
                        Register::VirtualRegister(virt) => *self.allocation_plan.get(virt).unwrap(),
                        Register::GlobalRegister(_) => todo!(),
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
