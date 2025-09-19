use {
    crate::{
        host::dbt::x86::encoder::{
            Instruction, Opcode, Operand, OperandKind, UseDef, UseDefMut,
            registers::{PhysicalRegister, Register},
            width::Width,
        },
        println,
    },
    alloc::vec::Vec,
    common::hashmap::{HashMap, HashSet},
};

use crate::host::dbt::Alloc as MemAlloc;

#[derive(Default, Clone, Debug)]
struct RegisterTrack {
    first_def: Option<usize>,
    last_use: Option<usize>,
    physical_register: Option<PhysicalRegister>,
    tracking: Option<Register>,
    interference: HashSet<PhysicalRegister>,
    last_control_flow_count: i32,
}

macro_rules! track_phys {
    ($map: ident, $name: expr) => {
        $map.insert(Register::Physical($name), RegisterTrack::default());
    };
}

pub fn allocate<A: MemAlloc>(
    instructions: &mut Vec<Instruction<A>>,
    num_virtual_registers: usize,
    global_register_offset: usize,
) {
    let mut register_tracking = HashMap::<Register, RegisterTrack>::default();

    track_phys!(register_tracking, PhysicalRegister::RAX);
    track_phys!(register_tracking, PhysicalRegister::RCX);
    track_phys!(register_tracking, PhysicalRegister::RDX);
    track_phys!(register_tracking, PhysicalRegister::RBX);
    track_phys!(register_tracking, PhysicalRegister::RSI);
    track_phys!(register_tracking, PhysicalRegister::RDI);
    track_phys!(register_tracking, PhysicalRegister::RBP);
    track_phys!(register_tracking, PhysicalRegister::R8);
    track_phys!(register_tracking, PhysicalRegister::R9);
    track_phys!(register_tracking, PhysicalRegister::R10);
    track_phys!(register_tracking, PhysicalRegister::R11);
    track_phys!(register_tracking, PhysicalRegister::R12);
    track_phys!(register_tracking, PhysicalRegister::R13);
    track_phys!(register_tracking, PhysicalRegister::R14);

    for vreg_index in 0..num_virtual_registers {
        register_tracking.insert(Register::Virtual(vreg_index), RegisterTrack::default());
    }

    //alloc::vec![VirtualRegister::default(); num_virtual_registers];

    calculate_vreg_live_ranges(&mut register_tracking, instructions);
    do_allocate(&mut register_tracking, instructions);
    commit(&register_tracking, instructions, global_register_offset);

    println!("{:?}", register_tracking);
}

fn calculate_vreg_live_ranges<A: MemAlloc>(
    register_tracking: &mut HashMap<Register, RegisterTrack>,
    instructions: &mut Vec<Instruction<A>>,
) {
    let mut current_control_flow_count: i32 = 0;

    for current in 0..instructions.len() {
        let instr = instructions[current];
        if matches!(instr.0, Opcode::JE(_) | Opcode::JNE(_) | Opcode::JMP(_)) {
            current_control_flow_count += 1;
        }

        instr.get_use_defs().for_each(|ud| {
            let (UseDef::Def(reg) | UseDef::UseDef(reg)) = ud.0 else {
                return;
            };

            let Register::Virtual(_) = reg else {
                return;
            };

            let tracked_register = register_tracking.get_mut(&reg).unwrap();

            if tracked_register.first_def.is_none() {
                tracked_register.first_def = Some(current);
                tracked_register.last_control_flow_count = current_control_flow_count;
            } else if tracked_register.last_use.is_none() && !matches!(ud.0, UseDef::UseDef(_)) {
                if tracked_register.last_control_flow_count == current_control_flow_count {
                    instructions[tracked_register.first_def.unwrap()].0 = Opcode::DEAD;
                    tracked_register.first_def = Some(current);
                }
            }
        });

        instr.get_use_defs().for_each(|ud| {
            let (UseDef::Use(reg) | UseDef::UseDef(reg)) = ud.0 else {
                return;
            };

            let Register::Virtual(_) = reg else {
                return;
            };

            register_tracking.get_mut(&reg).unwrap().last_use = Some(current);
        });
    }
}

fn do_allocate<A: MemAlloc>(
    register_tracking: &mut HashMap<Register, RegisterTrack>,
    instructions: &mut Vec<Instruction<A>>,
) {
    let mut avail_phys_regs = HashSet::<PhysicalRegister>::default();
    avail_phys_regs.insert(PhysicalRegister::RAX);
    avail_phys_regs.insert(PhysicalRegister::RCX);
    avail_phys_regs.insert(PhysicalRegister::RDX);
    avail_phys_regs.insert(PhysicalRegister::RBX);

    avail_phys_regs.insert(PhysicalRegister::RSI);
    avail_phys_regs.insert(PhysicalRegister::RDI);
    avail_phys_regs.insert(PhysicalRegister::R8);
    avail_phys_regs.insert(PhysicalRegister::R9);
    avail_phys_regs.insert(PhysicalRegister::R10);
    avail_phys_regs.insert(PhysicalRegister::R11);
    avail_phys_regs.insert(PhysicalRegister::R12);
    avail_phys_regs.insert(PhysicalRegister::R13);
    avail_phys_regs.insert(PhysicalRegister::R14);

    avail_phys_regs.insert(PhysicalRegister::RBP);

    let mut live_phys_regs = HashSet::<PhysicalRegister>::default();
    live_phys_regs.insert(PhysicalRegister::RBP);

    register_tracking
        .get_mut(&Register::Physical(PhysicalRegister::RBP))
        .unwrap()
        .tracking = Some(Register::Physical(PhysicalRegister::RBP));

    instructions
        .iter()
        .enumerate()
        .rev()
        .for_each(|(current_instruction_index, instruction)| {
            log::debug!("@ {} = {}", current_instruction_index, instruction);

            let mut skip = false;

            'usedef_iter: for usedef in instruction.get_use_defs() {
                let (UseDef::Def(usedef_reg) | UseDef::UseDef(usedef_reg)) = usedef.0 else {
                    continue;
                };

                match usedef_reg {
                    Register::Virtual(usedef_virt_reg) => {
                        // Definition of a virtual register
                        let tracked_virt_reg = register_tracking.get_mut(&usedef_reg).unwrap();

                        if tracked_virt_reg.first_def == Some(current_instruction_index) {
                            if tracked_virt_reg.last_use.is_none() {
                                log::debug!("definition of unused vreg {}", usedef_virt_reg);
                                skip = true;
                                break 'usedef_iter;
                            } else {
                                log::debug!(
                                    "ending live-range of vreg {} in preg {:?}",
                                    usedef_virt_reg,
                                    tracked_virt_reg.physical_register
                                );
                                live_phys_regs.remove(&tracked_virt_reg.physical_register.unwrap());
                            }
                        }
                    }
                    Register::Physical(usedef_phys_reg) => {
                        // Definition of a physical register
                        let tracked_phys_reg = register_tracking.get_mut(&usedef_reg).unwrap();

                        if live_phys_regs.contains(&usedef_phys_reg) {
                            live_phys_regs.remove(&usedef_phys_reg);

                            if let Some(Register::Virtual(conflicting_vreg_index)) =
                                tracked_phys_reg.tracking
                            {
                                log::debug!(
                                    "def of preg {}, but it's tracking vreg {}!",
                                    usedef_phys_reg,
                                    conflicting_vreg_index
                                );

                                // Allocate a physical register
                                let allocated_phys_reg = *avail_phys_regs
                                    .difference(
                                        &register_tracking
                                            .get(&Register::Virtual(conflicting_vreg_index))
                                            .unwrap()
                                            .interference,
                                    )
                                    .next()
                                    .unwrap();

                                register_tracking
                                    .get_mut(&Register::Virtual(conflicting_vreg_index))
                                    .unwrap()
                                    .physical_register = Some(allocated_phys_reg);
                                register_tracking
                                    .get_mut(&Register::Physical(allocated_phys_reg))
                                    .unwrap()
                                    .tracking = Some(Register::Virtual(conflicting_vreg_index));

                                live_phys_regs.insert(allocated_phys_reg);
                                register_tracking
                                    .get_mut(&Register::Virtual(conflicting_vreg_index))
                                    .unwrap()
                                    .interference = live_phys_regs.clone();

                                for avail_phys_reg in &avail_phys_regs {
                                    if live_phys_regs.contains(avail_phys_reg) {
                                        let avail_phys_reg_track = register_tracking
                                            .get(&Register::Physical(*avail_phys_reg))
                                            .unwrap();

                                        register_tracking
                                            .get_mut(&avail_phys_reg_track.tracking.unwrap())
                                            .unwrap()
                                            .interference
                                            .extend(&live_phys_regs);
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        // Do nothing for globals
                    }
                }
            }

            if skip {
                return;
            }

            for usedef in instruction.get_use_defs() {
                let (UseDef::Use(usedef_reg) | UseDef::UseDef(usedef_reg)) = usedef.0 else {
                    continue;
                };

                match usedef_reg {
                    Register::Virtual(usedef_virt_reg) => {
                        // Use of a virtual register
                        let tracked_virt_reg = register_tracking.get_mut(&usedef_reg).unwrap();

                        if tracked_virt_reg.last_use == Some(current_instruction_index)
                            || tracked_virt_reg.physical_register.is_none()
                        {
                            //  ALLOCATE
                            let allocated_phys_reg =
                                *avail_phys_regs.difference(&live_phys_regs).next().unwrap();

                            tracked_virt_reg.physical_register = Some(allocated_phys_reg);
                            live_phys_regs.insert(allocated_phys_reg);

                            tracked_virt_reg.interference = live_phys_regs.clone();

                            register_tracking
                                .get_mut(&Register::Physical(allocated_phys_reg))
                                .unwrap()
                                .tracking = Some(usedef_reg);

                            for avail_phys_reg in &avail_phys_regs {
                                if live_phys_regs.contains(avail_phys_reg) {
                                    let avail_phys_reg_track = register_tracking
                                        .get(&Register::Physical(*avail_phys_reg))
                                        .unwrap();

                                    //log::debug!(" updating preg={} vreg={}")

                                    register_tracking
                                        .get_mut(&avail_phys_reg_track.tracking.unwrap())
                                        .unwrap()
                                        .interference
                                        .extend(&live_phys_regs);
                                }
                            }

                            log::debug!(
                                "starting live-range of vreg {}, allocated to preg {}",
                                usedef_virt_reg,
                                allocated_phys_reg
                            );
                        }
                    }
                    Register::Physical(usedef_phys_reg) => {
                        // Use of a physical register
                        let tracked_phys_reg = register_tracking.get_mut(&usedef_reg).unwrap();

                        if live_phys_regs.contains(&usedef_phys_reg)
                            && tracked_phys_reg.tracking != Some(usedef_reg)
                        {
                            // let conflicting_vreg =
                            //     register_tracking.get(&tracked_phys_reg.tracking.unwrap());

                            log::debug!(
                                "conflicting use of preg {}, currently tracking {:?}",
                                usedef_phys_reg,
                                tracked_phys_reg.tracking
                            );

                            todo!()
                        } else {
                            register_tracking.get_mut(&usedef_reg).unwrap().tracking =
                                Some(usedef_reg);
                            live_phys_regs.insert(usedef_phys_reg);
                            register_tracking.get_mut(&usedef_reg).unwrap().interference =
                                live_phys_regs.clone();

                            for avail_phys_reg in &avail_phys_regs {
                                if live_phys_regs.contains(avail_phys_reg) {
                                    let avail_phys_reg_track = register_tracking
                                        .get(&Register::Physical(*avail_phys_reg))
                                        .unwrap();

                                    //log::debug!(" updating preg={} vreg={}")

                                    register_tracking
                                        .get_mut(&avail_phys_reg_track.tracking.unwrap())
                                        .unwrap()
                                        .interference
                                        .extend(&live_phys_regs);
                                }
                            }

                            log::debug!("starting live-range of preg {}", usedef_phys_reg);
                        }
                    }
                    _ => {
                        // Do nothing for globals
                    }
                }
            }
        });
}

fn commit<A: MemAlloc>(
    register_tracking: &HashMap<Register, RegisterTrack>,
    instructions: &mut Vec<Instruction<A>>,
    global_register_offset: usize,
) {
    instructions.iter_mut().for_each(|instruction| {
        if matches!(instruction.0, Opcode::DEAD) {
            return;
        }

        instruction.get_operands_mut().for_each(|op| {
            if let Some((_, op)) = op {
                if let OperandKind::Register(Register::Global(idx)) = op.kind() {
                    *op = Operand::mem_base_displ(
                        op.width(),
                        Register::Physical(PhysicalRegister::RBP),
                        i32::try_from(global_register_offset + (*idx * 8)).unwrap(),
                    )
                }
            }
        });

        instruction.get_use_defs_mut().for_each(|ud| {
            let (UseDefMut::Def(reg) | UseDefMut::Use(reg) | UseDefMut::UseDef(reg)) = ud;
            if let Register::Virtual(vreg) = &*reg {
                *reg = Register::Physical(
                    register_tracking
                        .get(&Register::Virtual(*vreg))
                        .unwrap()
                        .physical_register
                        .unwrap(),
                );

                // Register::Physical(*allocation_plan.get(vreg).unwrap());
            }
        });
    });

    // kill redundant mov's
    instructions.iter_mut().for_each(|instruction| {
        if let Opcode::MOV(src, dst) = instruction.0 {
            if src == dst && src.width_in_bits != Width::_32 {
                instruction.0 = Opcode::DEAD;
            }
        }
    });
}
