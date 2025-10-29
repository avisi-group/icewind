use {
    crate::{
        bump_alloc::BumpAllocatorRef,
        emitter::Emitter,
        register_file::GLOBAL_REGISTER_SIZE,
        x86::{
            emitter::{X86Emitter, X86NodeRef},
            encoder::{Instruction, Opcode, Operand, OperandKind, registers::PhysicalRegister},
        },
    },
    alloc::{collections::vec_deque::VecDeque, vec::Vec},
    common::{
        arena::{Arena, Ref},
        hashmap::{HashMap, HashMapA, hashmap_in, hashset_in},
        intern::InternedString,
        rudder::Model,
    },
    core::fmt::Debug,
    iced_x86::code_asm::CodeAssembler,
};

pub mod dot;
pub mod emitter;
pub mod encoder;
pub mod register_allocator;

pub const TRACING_ENABLED: bool = false;

// sysv64 ABI
pub const ARG_REGS: &[PhysicalRegister] = &[
    PhysicalRegister::RDI,
    PhysicalRegister::RSI,
    PhysicalRegister::RDX,
    PhysicalRegister::RCX,
    PhysicalRegister::R8,
    PhysicalRegister::R9,
];

pub const CALLER_SAVED: &[PhysicalRegister] = &[
    PhysicalRegister::RAX,
    PhysicalRegister::RDI,
    PhysicalRegister::RSI,
    PhysicalRegister::RDX,
    PhysicalRegister::RCX,
    PhysicalRegister::R8,
    PhysicalRegister::R9,
    PhysicalRegister::R10,
    PhysicalRegister::R11,
];

#[derive(Debug, Clone, Copy)]
pub enum X86BlockMark {
    None,
    Temporary,
    Permanent,
}

pub struct X86Block {
    instructions: Vec<Instruction, BumpAllocatorRef>,
    next: Vec<Ref<X86Block>, BumpAllocatorRef>,
    linked: bool,
    mark: X86BlockMark,
}

impl X86Block {
    pub fn new_in(allocator: BumpAllocatorRef) -> Self {
        Self {
            instructions: Vec::new_in(allocator.clone()),
            next: Vec::new_in(allocator),
            linked: false,
            mark: X86BlockMark::None,
        }
    }

    pub fn set_linked(&mut self) {
        self.linked = true;
    }

    pub fn is_linked(&self) -> bool {
        self.linked
    }

    pub fn set_mark(&mut self, mark: X86BlockMark) {
        self.mark = mark;
    }

    pub fn get_mark(&self) -> X86BlockMark {
        self.mark
    }

    pub fn append(&mut self, instruction: Instruction) {
        self.instructions.push(instruction);
    }

    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    pub fn instructions_mut(&mut self) -> &mut Vec<Instruction, BumpAllocatorRef> {
        &mut self.instructions
    }

    pub fn next_blocks(&self) -> &[Ref<X86Block>] {
        &self.next
    }

    pub fn clear_next_blocks(&mut self) {
        self.next.clear();
    }

    pub fn push_next(&mut self, target: Ref<X86Block>) {
        self.next.push(target);
        if self.next.len() > 2 {
            panic!(
                "bad, blocks should not have more than 2 real targets (asserts complicate things)"
            )
        }
    }
}

impl Debug for X86Block {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for instr in &self.instructions {
            writeln!(f, "\t{instr}")?;
        }

        Ok(())
    }
}

struct CachedFunction {
    entry_block: Ref<X86Block>,
    result: Option<X86NodeRef>,
}

pub struct X86TranslationContext {
    allocator: BumpAllocatorRef,
    blocks: Arena<X86Block, BumpAllocatorRef>,
    initial_block: Ref<X86Block>,
    panic_block: Ref<X86Block>,
    writes_to_pc: bool,

    function_cache: HashMapA<InternedString, CachedFunction, BumpAllocatorRef>,

    pc_offset: u64,
    el_offset: u64,
    sctlr_el1_offset: u64,
    ttbr0_el1_offset: u64,
    ttbr1_el1_offset: u64,
    n_offset: u64,
    z_offset: u64,
    c_offset: u64,
    v_offset: u64,

    callbacks: Callbacks,

    global_register_offset: usize,

    /// Counter for allocating variable ids
    current_variable_id: usize,

    memory_mask: bool,
}

impl Debug for X86TranslationContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "X86TranslationContext:")?;
        writeln!(f, "\tinitial: {:?}", self.initial_block())?;
        writeln!(f, "\tpanic: {:?}", self.panic_block())?;
        writeln!(f)?;

        let mut visited = hashset_in(self.allocator());
        let mut to_visit = Vec::new_in(self.allocator());
        to_visit.push(self.initial_block());

        while let Some(next) = to_visit.pop() {
            writeln!(f, "{next:x?}:")?;
            for instr in next.get(self.arena()).instructions() {
                writeln!(f, "\t{instr}")?;
            }

            visited.insert(next);

            to_visit.extend(
                next.get(self.arena())
                    .next_blocks()
                    .iter()
                    .filter(|b| !visited.contains(*b)),
            );
        }

        Ok(())
    }
}

// impl X86TranslationContext {
//     pub fn new(
//         model: &Model,
//         memory_mask: bool,
//         global_register_offset: usize,
//         el_changed_callback: extern "sysv64" fn(u8, u8),
//     ) -> Self {
//         Self::new_with_allocator(
//             Global,
//             model,
//             memory_mask,
//             global_register_offset,
//             el_changed_callback,
//         )
//     }
// }

pub struct Callbacks {
    pub el_changed_callback: extern "sysv64" fn(u8, u8),
    pub trace_instruction_start: extern "sysv64" fn(u32, u64),
    pub trace_instruction_end: extern "sysv64" fn(),
    pub trace_register_read: extern "sysv64" fn(u64, u64),
    pub trace_register_write: extern "sysv64" fn(u64, u64),
    pub trace_memory_read: extern "sysv64" fn(u64, u64, u8),
    pub trace_memory_write: extern "sysv64" fn(u64, u64, u8),
}

impl<'a> X86TranslationContext {
    pub fn new_with_allocator(
        allocator: BumpAllocatorRef,
        model: &Model,
        memory_mask: bool,
        global_register_offset: usize,
        callbacks: Callbacks,
    ) -> Self {
        let mut arena = Arena::new_in(allocator.clone());

        let initial_block = arena.insert(X86Block::new_in(allocator.clone()));
        let panic_block = arena.insert(X86Block::new_in(allocator.clone()));

        let mut celf = Self {
            allocator,
            blocks: arena,
            initial_block,
            panic_block,
            writes_to_pc: false,
            function_cache: hashmap_in(allocator),

            pc_offset: model.reg_offset("_PC"),
            el_offset: model.reg_offset("PSTATE_EL"),
            sctlr_el1_offset: model.reg_offset("SCTLR_EL1_bits"),
            ttbr0_el1_offset: model.reg_offset("_TTBR0_EL1_bits"),
            ttbr1_el1_offset: model.reg_offset("_TTBR1_EL1_bits"),
            n_offset: model.reg_offset("PSTATE_N"),
            z_offset: model.reg_offset("PSTATE_Z"),
            c_offset: model.reg_offset("PSTATE_C"),
            v_offset: model.reg_offset("PSTATE_V"),
            global_register_offset,
            memory_mask,
            current_variable_id: 0,

            callbacks,
        };

        // add panic to the panic block
        {
            let mut emitter = X86Emitter::new(&mut celf);
            emitter.set_current_block(panic_block);
            emitter.panic("panic block");
        }

        celf
    }

    pub fn allocator(&self) -> BumpAllocatorRef {
        self.allocator.clone()
    }

    pub fn arena(&self) -> &Arena<X86Block, BumpAllocatorRef> {
        &self.blocks
    }

    pub fn arena_mut(&mut self) -> &mut Arena<X86Block, BumpAllocatorRef> {
        &mut self.blocks
    }

    fn initial_block(&self) -> Ref<X86Block> {
        self.initial_block
    }

    pub fn panic_block(&self) -> Ref<X86Block> {
        self.panic_block
    }

    pub fn compile(mut self, num_virtual_registers: usize) -> Vec<u8> {
        let mut assembler = CodeAssembler::new(64).unwrap();

        //let mut label_map = hashmap_in(self.allocator());

        log::trace!("building work queue");

        let all_blocks = {
            let mut all_blocks = Vec::new_in(self.allocator());
            let mut work_queue = Vec::new_in(self.allocator());
            work_queue.push(self.panic_block());
            work_queue.push(self.initial_block());

            while let Some(block) = work_queue.pop() {
                if !block.get(self.arena()).is_linked() {
                    block.get_mut(self.arena_mut()).set_linked();

                    all_blocks.push(block);

                    empty_block_jump_threading(self.arena_mut(), block);
                    for block in block.get(self.arena()).next_blocks() {
                        work_queue.push(*block);
                    }
                }
            }

            all_blocks
        };

        log::debug!("{}", dot::render(self.arena(), self.initial_block()));

        let mut instructions = Vec::<Instruction>::new();
        let mut block_to_instruction_index = hashmap_in(self.allocator());
        for block in &all_blocks {
            block_to_instruction_index.insert(block.clone(), instructions.len());
            instructions.extend(block.get(self.arena()).instructions());
        }

        instructions
            .iter_mut()
            .enumerate()
            .for_each(|(idx, instr)| {
                if let Opcode::JMP(target) = instr.0 {
                    let OperandKind::Target(target) = target.kind() else {
                        return;
                    };

                    let target_index = block_to_instruction_index.get(target).unwrap();

                    if *target_index == idx + 1 {
                        instr.0 = Opcode::DEAD;
                    }
                }
            });

        log::trace!("\n\n\nPRE ALLOC");
        for (idx, instr) in instructions.iter().enumerate() {
            log::trace!("{idx}: {instr}");
        }

        register_allocator::allocate(
            &mut instructions,
            num_virtual_registers,
            self.global_register_offset,
        );

        log::trace!("\n\n\nPOST ALLOC");
        for (idx, instr) in instructions.iter().enumerate() {
            log::trace!("{idx}: {instr}");
        }

        // Collapse labels
        // Go through each instruction
        // Check if instruction is target
        // Check if not "not dead"
        // If dead only, remove label

        let mut instruction_labels = HashMap::default();
        let mut block_labels = hashmap_in(self.allocator());

        log::debug!("jumps:");
        for (idx, instr) in instructions.iter().enumerate() {
            log::debug!("{}: {}", idx, instr);

            match instr.0 {
                Opcode::JE(Operand {
                    kind: OperandKind::Target(target),
                    ..
                })
                | Opcode::JNE(Operand {
                    kind: OperandKind::Target(target),
                    ..
                })
                | Opcode::JMP(Operand {
                    kind: OperandKind::Target(target),
                    ..
                }) => {
                    let target_index = block_to_instruction_index.get(&target).unwrap();
                    log::debug!("  jump to {}", target_index);

                    let next_valid = instructions
                        .iter()
                        .enumerate()
                        .skip(*target_index)
                        .find(|(_, instr)| !matches!(instr.0, Opcode::DEAD))
                        .unwrap();

                    log::debug!("  skipped to {}", next_valid.0);

                    let label = instruction_labels
                        .entry(next_valid.0)
                        .or_insert_with(|| assembler.create_label());

                    block_labels.insert(target, label.clone());
                }
                _ => {}
            }
        }

        log::trace!("encoding instructions");
        for (idx, instr) in instructions.iter().enumerate() {
            if let Some(instruction_label) = instruction_labels.get_mut(&idx) {
                assembler.set_label(instruction_label).unwrap();
            }

            instr.encode(&mut assembler, &block_labels);
        }

        // for (i, block) in all_blocks.iter().enumerate() {
        //     let block_label = label_map.get_mut(block).unwrap();
        //     if let Err(e) = assembler.set_label(block_label) {
        //         // If there is already an active label, then emit a nop and try
        // again.         assembler.nop().unwrap();

        //         // I don't think there is a better way to do this yet, without some
        //         // significant re-thinking.  This is because, we pre-create the block
        //         // labels, but if we jump forward to a block label, which we then
        // don't         // use (because it aliases), then we've already emitted
        // a jump to the         // unused label.

        //         assembler.set_label(block_label).unwrap_or_else(|e| {
        //             panic!(
        //                 "{e}: label already set OR label {:?} for block {block:?}
        // re-used",                 label_map.get_mut(block).unwrap()
        //             );
        //         });
        //     }

        //     // assembler
        //     //
        // .nop_1::<iced_x86::code_asm::AsmMemoryOperand>(iced_x86::code_asm::qword_ptr(
        //     //
        // iced_x86::code_asm::AsmRegister64::from(iced_x86::code_asm::rax)
        //     //             + block.index(),
        //     //     ))
        //     //     .unwrap();

        //     let instrs = block.get(self.arena()).instructions();

        //     let (last, rest) = instrs.split_last().unwrap_or_else(|| {
        //         panic!(
        //             "block {:?} {block:?} was empty",
        //             label_map.get_mut(block).unwrap()
        //         )
        //     });

        //     // all but last
        //     for instr in rest {
        //         instr.encode(&mut assembler, &label_map);
        //     }

        //     assert!(matches!(
        //         last,
        //         Instruction(Opcode::JMP(_) | Opcode::INT(_) | Opcode::RET)
        //     ));

        //     // fallthrough jump optimization
        //     if let Instruction(Opcode::JMP(op)) = last {
        //         if let OperandKind::Target(target) = op.kind() {
        //             if all_blocks.get(i + 1).copied() == Some(*target) {
        //                 // do not emit jump
        //                 continue;
        //             }
        //         }
        //     }

        //     last.encode(&mut assembler, &label_map);
        // }

        log::trace!("assembling");
        let code = assembler.assemble(0).unwrap();

        code
    }

    pub fn create_block(&mut self) -> Ref<X86Block> {
        let b = X86Block::new_in(self.allocator());
        self.arena_mut().insert(b)
    }

    /// Sets the "PC was written to" flag
    pub fn set_pc_write_flag(&mut self) {
        self.writes_to_pc = true;
    }

    /// Gets the value of the "PC was written to" flag
    pub fn get_pc_write_flag(&self) -> bool {
        self.writes_to_pc
    }

    pub fn pc_offset(&self) -> u64 {
        self.pc_offset
    }

    pub fn allocate_variable_id(&mut self) -> usize {
        let id = self.current_variable_id;

        self.current_variable_id += 1;

        // todo: be better about this
        if id >= GLOBAL_REGISTER_SIZE / 16 {
            panic!("variable number {id:#x} exceeded MAX_STACK_SIZE ({GLOBAL_REGISTER_SIZE:#x})")
        }

        id
    }
}

fn link_visit(
    block: Ref<X86Block>,
    arena: &mut Arena<X86Block>,
    sorted_blocks: &mut VecDeque<Ref<X86Block>>,
) -> bool {
    match block.get(arena).get_mark() {
        X86BlockMark::Permanent => true,
        X86BlockMark::Temporary => false,
        X86BlockMark::None => {
            block.get_mut(arena).set_mark(X86BlockMark::Temporary);

            for next_block in block
                .get(arena)
                .next_blocks()
                .iter()
                .copied()
                .collect::<Vec<_>>()
            {
                if !link_visit(next_block, arena, sorted_blocks) {
                    return false;
                }
            }

            block.get_mut(arena).set_mark(X86BlockMark::Permanent);

            sorted_blocks.push_front(block);

            true
        }
    }
}

fn empty_block_jump_threading(
    arena: &mut Arena<X86Block, BumpAllocatorRef>,
    current_block: Ref<X86Block>,
) {
    // if the current block only has one target
    if let [child] = current_block.get(arena).next_blocks() {
        // and that target only has a single instruction (a jump)
        if let [Instruction(Opcode::JMP(op))] = child.get(arena).instructions() {
            let op = *op;

            // replace the jump in the current block with the jump of the child
            *current_block
                .get_mut(arena)
                .instructions_mut()
                .last_mut()
                .unwrap() = Instruction(Opcode::JMP(op));

            let OperandKind::Target(grandchild) = op.kind() else {
                unreachable!();
            };

            // replace the child block in the current block's "next blocks" with the
            // grandchild block
            current_block.get_mut(arena).clear_next_blocks();
            current_block.get_mut(arena).push_next(*grandchild);

            // recurse
            empty_block_jump_threading(arena, current_block);
        }
    }
}
