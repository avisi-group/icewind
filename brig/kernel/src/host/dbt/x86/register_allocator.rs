use {
    alloc::vec::Vec,
    dbt::x86::{
        ARG_REGS, CALLER_SAVED,
        encoder::{
            Instruction, Opcode, Operand,
            OperandKind::{self},
            UseDef, UseDefMut,
            registers::{PhysicalRegister, Register},
            width::Width,
        },
    },
    strum::EnumCount,
};

use shared::Alloc as MemAlloc;

#[derive(Default, Clone, Debug)]
struct RegisterTrack {
    first_def: Option<usize>,
    last_use: Option<usize>,
    physical_register: Option<PhysicalRegister>,
    tracking: Option<Register>,
    interference: PhysicalRegisterSet,
    last_control_flow_count: i32,
}

pub fn allocate<A: MemAlloc>(
    instructions: &mut Vec<Instruction<A>>,
    num_virtual_registers: usize,
    global_register_offset: usize,
) {
    let mut register_tracking = RegisterTracker::new(num_virtual_registers);

    calculate_vreg_live_ranges(&mut register_tracking, instructions);

    let live_at_calls = do_allocate(&mut register_tracking, instructions);

    commit(
        &register_tracking,
        live_at_calls,
        instructions,
        global_register_offset,
    );
}

fn calculate_vreg_live_ranges<A: MemAlloc>(
    register_tracking: &mut RegisterTracker,
    instructions: &mut Vec<Instruction<A>>,
) {
    let mut current_control_flow_count: i32 = 0;

    for current in 0..instructions.len() {
        let instr = instructions[current];
        if matches!(instr.0, Opcode::JE(_) | Opcode::JNE(_) | Opcode::JMP(_)) {
            current_control_flow_count += 1;
        }

        instr
            .get_use_defs()
            .filter_map(|ud| {
                if let UseDef::Def(reg) | UseDef::UseDef(reg) = ud.0 {
                    Some((ud.0, reg))
                } else {
                    None
                }
            })
            .filter(|(_, reg)| matches!(reg, Register::Virtual(_)))
            .for_each(|(ud, reg)| {
                let tracked_register = register_tracking.get_mut(&reg);

                if tracked_register.first_def.is_none() {
                    tracked_register.first_def = Some(current);
                    tracked_register.last_control_flow_count = current_control_flow_count;
                } else if matches!(instr.0, Opcode::CMOVNE(_, _)) {
                    tracked_register.last_use = Some(current);
                } else if tracked_register.last_use.is_none() && !matches!(ud, UseDef::UseDef(_)) {
                    if tracked_register.last_control_flow_count == current_control_flow_count {
                        instructions[tracked_register.first_def.unwrap()].0 = Opcode::DEAD;
                        tracked_register.first_def = Some(current);
                    }
                }
            });

        instr
            .get_use_defs()
            .filter_map(|ud| {
                if let UseDef::Use(reg) | UseDef::UseDef(reg) = ud.0 {
                    Some(reg)
                } else {
                    None
                }
            })
            .filter(|reg| matches!(reg, Register::Virtual(_)))
            .for_each(|reg| {
                register_tracking.get_mut(&reg).last_use = Some(current);
            });
    }
}

fn do_allocate<A: MemAlloc>(
    register_tracking: &mut RegisterTracker,
    instructions: &mut Vec<Instruction<A>>,
) -> Vec<(usize, PhysicalRegisterSet)> {
    let mut avail_phys_regs_gpr = PhysicalRegisterSet::new();
    let mut avail_phys_regs_xmm = PhysicalRegisterSet::new();

    avail_phys_regs_gpr.insert(PhysicalRegister::RAX);
    avail_phys_regs_gpr.insert(PhysicalRegister::RCX);
    avail_phys_regs_gpr.insert(PhysicalRegister::RDX);
    avail_phys_regs_gpr.insert(PhysicalRegister::RBX);
    avail_phys_regs_gpr.insert(PhysicalRegister::RSI);
    avail_phys_regs_gpr.insert(PhysicalRegister::RDI);
    avail_phys_regs_gpr.insert(PhysicalRegister::R8);
    avail_phys_regs_gpr.insert(PhysicalRegister::R9);
    avail_phys_regs_gpr.insert(PhysicalRegister::R10);
    avail_phys_regs_gpr.insert(PhysicalRegister::R11);
    avail_phys_regs_gpr.insert(PhysicalRegister::R12);
    avail_phys_regs_gpr.insert(PhysicalRegister::R13);
    avail_phys_regs_gpr.insert(PhysicalRegister::R14);

    avail_phys_regs_xmm.insert(PhysicalRegister::XMM0);
    avail_phys_regs_xmm.insert(PhysicalRegister::XMM1);
    avail_phys_regs_xmm.insert(PhysicalRegister::XMM2);
    avail_phys_regs_xmm.insert(PhysicalRegister::XMM3);
    avail_phys_regs_xmm.insert(PhysicalRegister::XMM4);
    avail_phys_regs_xmm.insert(PhysicalRegister::XMM5);
    avail_phys_regs_xmm.insert(PhysicalRegister::XMM6);
    avail_phys_regs_xmm.insert(PhysicalRegister::XMM7);

    let mut live_phys_regs = PhysicalRegisterSet::new();

    // store what physical registers are live at a given call
    let mut live_at_calls = Vec::new();

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

                log::debug!("ud def: {usedef_reg:?}");

                match usedef_reg {
                    Register::Virtual(usedef_virt_reg) => {
                        // Definition of a virtual register
                        let tracked_virt_reg = register_tracking.get_mut(&usedef_reg);

                        log::debug!("tracked virt reg: {tracked_virt_reg:?}");

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
                                live_phys_regs.remove(tracked_virt_reg.physical_register.unwrap());
                            }
                        }
                    }
                    Register::Physical(usedef_phys_reg) => {
                        // Definition of a physical register
                        let tracked_phys_reg = register_tracking.get_mut(&usedef_reg);

                        if live_phys_regs.contains(usedef_phys_reg) {
                            live_phys_regs.remove(usedef_phys_reg);

                            if let Some(Register::Virtual(conflicting_vreg_index)) =
                                tracked_phys_reg.tracking
                            {
                                log::debug!(
                                    "def of preg {}, but it's tracking vreg {}!",
                                    usedef_phys_reg,
                                    conflicting_vreg_index
                                );

                                // Allocate a physical register
                                let allocated_phys_reg = allocate_phys_reg(
                                    usedef.1,
                                    &avail_phys_regs_xmm,
                                    &avail_phys_regs_gpr,
                                    &register_tracking
                                        .get(&Register::Virtual(conflicting_vreg_index))
                                        .interference,
                                );

                                register_tracking
                                    .get_mut(&Register::Virtual(conflicting_vreg_index))
                                    .physical_register = Some(allocated_phys_reg);
                                register_tracking
                                    .get_mut(&Register::Physical(allocated_phys_reg))
                                    .tracking = Some(Register::Virtual(conflicting_vreg_index));

                                live_phys_regs.insert(allocated_phys_reg);
                                register_tracking
                                    .get_mut(&Register::Virtual(conflicting_vreg_index))
                                    .interference = live_phys_regs;

                                for avail_phys_reg in
                                    avail_phys_regs_gpr.iter().chain(avail_phys_regs_xmm.iter())
                                {
                                    if live_phys_regs.contains(avail_phys_reg) {
                                        let avail_phys_reg_track = register_tracking
                                            .get(&Register::Physical(avail_phys_reg));

                                        register_tracking
                                            .get_mut(&avail_phys_reg_track.tracking.unwrap())
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

                log::debug!("ud use: {usedef_reg:?}");

                match usedef_reg {
                    Register::Virtual(usedef_virt_reg) => {
                        // Use of a virtual register
                        let tracked_virt_reg = register_tracking.get_mut(&usedef_reg);

                        if tracked_virt_reg.last_use == Some(current_instruction_index)
                            && tracked_virt_reg.physical_register.is_none()
                        {
                            //  ALLOCATE
                            let allocated_phys_reg = allocate_phys_reg(
                                usedef.1,
                                &avail_phys_regs_xmm,
                                &avail_phys_regs_gpr,
                                &live_phys_regs,
                            );

                            tracked_virt_reg.physical_register = Some(allocated_phys_reg);
                            live_phys_regs.insert(allocated_phys_reg);

                            tracked_virt_reg.interference = live_phys_regs;

                            register_tracking
                                .get_mut(&Register::Physical(allocated_phys_reg))
                                .tracking = Some(usedef_reg);

                            for avail_phys_reg in
                                avail_phys_regs_gpr.iter().chain(avail_phys_regs_xmm.iter())
                            {
                                if live_phys_regs.contains(avail_phys_reg) {
                                    let avail_phys_reg_track =
                                        register_tracking.get(&Register::Physical(avail_phys_reg));

                                    //log::debug!(" updating preg={} vreg={}")

                                    register_tracking
                                        .get_mut(&avail_phys_reg_track.tracking.unwrap())
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
                        //let tracked_phys_reg = register_tracking.get(&usedef_reg).unwrap();

                        if live_phys_regs.contains(usedef_phys_reg)
                            && register_tracking.get(&usedef_reg).tracking != Some(usedef_reg)
                        {
                            let tracking_reg = register_tracking.get(&usedef_reg).tracking.unwrap();

                            let conflicting_vreg = register_tracking.get_mut(&tracking_reg);

                            log::debug!(
                                "conflicting use of preg {}, currently tracking {:?}",
                                usedef_phys_reg,
                                tracking_reg
                            );

                            let new_phys_reg = allocate_phys_reg(
                                usedef.1,
                                &avail_phys_regs_xmm,
                                &avail_phys_regs_gpr,
                                &conflicting_vreg.interference,
                            );

                            log::debug!(
                                "re-assigning vreg {} to preg {}",
                                tracking_reg,
                                new_phys_reg
                            );

                            conflicting_vreg.physical_register = Some(new_phys_reg);
                            live_phys_regs.insert(new_phys_reg);
                            conflicting_vreg.interference = live_phys_regs.clone();

                            register_tracking
                                .get_mut(&Register::Physical(new_phys_reg))
                                .tracking = Some(tracking_reg);

                            //  Update interferences
                            for avail_phys_reg in
                                avail_phys_regs_gpr.iter().chain(avail_phys_regs_xmm.iter())
                            {
                                if live_phys_regs.contains(avail_phys_reg) {
                                    let avail_phys_reg_track =
                                        register_tracking.get(&Register::Physical(avail_phys_reg));

                                    //log::debug!(" updating preg={} vreg={}")

                                    register_tracking
                                        .get_mut(&avail_phys_reg_track.tracking.unwrap())
                                        .interference
                                        .extend(&live_phys_regs);
                                }
                            }

                            register_tracking.get_mut(&usedef_reg).tracking = Some(usedef_reg);
                        } else {
                            register_tracking.get_mut(&usedef_reg).tracking = Some(usedef_reg);
                            live_phys_regs.insert(usedef_phys_reg);
                            register_tracking.get_mut(&usedef_reg).interference =
                                live_phys_regs.clone();

                            for avail_phys_reg in
                                avail_phys_regs_gpr.iter().chain(avail_phys_regs_xmm.iter())
                            {
                                if live_phys_regs.contains(avail_phys_reg) {
                                    let avail_phys_reg_track =
                                        register_tracking.get(&Register::Physical(avail_phys_reg));

                                    //log::debug!(" updating preg={} vreg={}")

                                    register_tracking
                                        .get_mut(&avail_phys_reg_track.tracking.unwrap())
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

            if matches!(instruction.0, Opcode::CALL { .. }) {
                live_at_calls.push((current_instruction_index, live_phys_regs.clone()));
            }
        });

    live_at_calls
}

fn commit<A: MemAlloc>(
    register_tracking: &RegisterTracker,
    mut live_at_calls: Vec<(usize, PhysicalRegisterSet)>,
    instructions: &mut Vec<Instruction<A>>,
    global_register_offset: usize,
) {
    for i in 0..instructions.len() {
        if matches!(instructions[i].0, Opcode::DEAD) {
            continue;
        }

        instructions[i].get_operands_mut().for_each(|op| {
            if let Some((_, op)) = op {
                if let OperandKind::Register(Register::Global(idx)) = op.kind() {
                    *op = Operand::mem_base_displ(
                        op.width(),
                        Register::Physical(PhysicalRegister::RBP),
                        i32::try_from(global_register_offset + (*idx * 16)).unwrap(),
                    )
                }
            }
        });

        instructions[i].get_use_defs_mut().for_each(|ud| {
            let (UseDefMut::Def(reg) | UseDefMut::Use(reg) | UseDefMut::UseDef(reg)) = ud;
            if let Register::Virtual(vreg) = &*reg {
                *reg = Register::Physical(
                    register_tracking
                        .get(&Register::Virtual(*vreg))
                        .physical_register
                        .unwrap(),
                );
            }
        });

        // kill redundant mov's
        if let Opcode::MOV(src, dst) = instructions[i].0 {
            if src == dst && src.width_in_bits != Width::_32 {
                instructions[i].0 = Opcode::DEAD;
            }
        }

        // insert call saves
        if let Instruction(Opcode::CALL {
            function,
            nr_input_args,
            ..
        }) = instructions[i]
        {
            let Some(Register::Physical(fn_ptr_register)) = function.as_register() else {
                panic!()
            };

            let Some((popped_idx, live_registers)) = live_at_calls.pop() else {
                panic!()
            };

            assert_eq!(i, popped_idx);

            let to_save = CALLER_SAVED
                .iter()
                // only save live registers
                .filter(|r| live_registers.contains(**r))
                // don't save in-use argument registers
                .filter(|r| {
                    ARG_REGS
                        .iter()
                        .take(nr_input_args)
                        .find(|c| **c == **r)
                        .is_none()
                })
                // don't save the fn ptr register
                .filter(|r| **r != fn_ptr_register)
                .copied()
                .collect::<Vec<_>>();

            for (sub_index, reg) in to_save.iter().enumerate() {
                instructions[i - (CALLER_SAVED.len() - sub_index)] =
                    Instruction::push(Operand::preg(Width::_64, *reg));
            }

            for (sub_index, reg) in to_save.iter().enumerate().rev() {
                instructions[i + (CALLER_SAVED.len() - sub_index)] =
                    Instruction::pop(Operand::preg(Width::_64, *reg));
            }
        }
    }
}

fn allocate_phys_reg(
    width: Width,
    xmm_avail: &PhysicalRegisterSet,
    gpr_avail: &PhysicalRegisterSet,
    in_use: &PhysicalRegisterSet,
) -> PhysicalRegister {
    (if width == Width::_128 {
        xmm_avail
    } else {
        gpr_avail
    })
    .first_difference(in_use)
    .unwrap()
}

struct RegisterTracker {
    virt: Vec<RegisterTrack>,
    phys: [RegisterTrack; PhysicalRegister::COUNT],
}

impl RegisterTracker {
    pub fn new(num_virtual_registers: usize) -> Self {
        Self {
            virt: alloc::vec![RegisterTrack::default(); num_virtual_registers],
            phys: Default::default(),
        }
    }

    pub fn get(&self, register: &Register) -> &RegisterTrack {
        match register {
            Register::Physical(physical_register) => &self.phys[physical_register.index()],
            Register::Virtual(vreg) => &self.virt[*vreg],
            Register::Global(_) => panic!(),
        }
    }

    pub fn get_mut(&mut self, register: &Register) -> &mut RegisterTrack {
        match register {
            Register::Physical(physical_register) => &mut self.phys[physical_register.index()],
            Register::Virtual(vreg) => &mut self.virt[*vreg],
            Register::Global(_) => panic!(),
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct PhysicalRegisterSet {
    set: [bool; PhysicalRegister::COUNT],
}

impl PhysicalRegisterSet {
    pub fn new() -> Self {
        Self {
            set: Default::default(),
        }
    }

    pub fn insert(&mut self, phys: PhysicalRegister) {
        self.set[phys.index()] = true;
    }

    pub fn remove(&mut self, phys: PhysicalRegister) {
        self.set[phys.index()] = false;
    }

    pub fn contains(&self, phys: PhysicalRegister) -> bool {
        self.set[phys.index()]
    }

    pub fn first_difference(&self, other: &Self) -> Option<PhysicalRegister> {
        self.set
            .iter()
            .enumerate()
            .zip(other.set.iter())
            .find(|((_, this), other)| **this && !**other)
            .map(|((idx, _), _)| PhysicalRegister::from_index(idx))
    }

    pub fn extend(&mut self, other: &Self) {
        self.set
            .iter_mut()
            .zip(other.set.iter())
            .for_each(|(this, other)| {
                if *other {
                    *this = true;
                }
            });
    }

    pub fn iter(&self) -> PhysicalRegisterSetIter {
        PhysicalRegisterSetIter {
            pos: 0,
            set: self.clone(),
        }
    }
}

struct PhysicalRegisterSetIter {
    pos: usize,
    set: PhysicalRegisterSet,
}

impl Iterator for PhysicalRegisterSetIter {
    type Item = PhysicalRegister;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.pos == self.set.set.len() {
                return None;
            }

            let next = self.set.set[self.pos];

            if !next {
                self.pos += 1;
            } else {
                let result = Some(PhysicalRegister::from_index(self.pos));
                self.pos += 1;
                return result;
            }
        }
    }
}
