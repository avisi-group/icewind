use {
    crate::x86::encoder::{Instruction, registers::PhysicalRegister},
    alloc::vec::Vec,
    brig_common::Alloc,
    common::arena::Ref,
    core::fmt::Debug,
};

pub mod encoder;

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

pub struct X86Block<A: Alloc> {
    instructions: Vec<Instruction<A>, A>,
    next: Vec<Ref<X86Block<A>>, A>,
    linked: bool,
    mark: X86BlockMark,
}

impl<A: Alloc> X86Block<A> {
    pub fn new_in(allocator: A) -> Self {
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

    pub fn append(&mut self, instruction: Instruction<A>) {
        self.instructions.push(instruction);
    }

    pub fn instructions(&self) -> &[Instruction<A>] {
        &self.instructions
    }

    pub fn instructions_mut(&mut self) -> &mut Vec<Instruction<A>, A> {
        &mut self.instructions
    }

    pub fn next_blocks(&self) -> &[Ref<X86Block<A>>] {
        &self.next
    }

    pub fn clear_next_blocks(&mut self) {
        self.next.clear();
    }

    pub fn push_next(&mut self, target: Ref<X86Block<A>>) {
        self.next.push(target);
        if self.next.len() > 2 {
            panic!(
                "bad, blocks should not have more than 2 real targets (asserts complicate things)"
            )
        }
    }
}

impl<A: Alloc> Debug for X86Block<A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for instr in &self.instructions {
            writeln!(f, "\t{instr}")?;
        }

        Ok(())
    }
}
