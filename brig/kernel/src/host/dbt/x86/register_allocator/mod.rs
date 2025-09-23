use {
    crate::host::dbt::{
        Alloc as MemAlloc,
        x86::encoder::{
            Instruction, Opcode, Operand,
            registers::{PhysicalRegister, Register},
            width::Width,
        },
    },
    alloc::{alloc::Global, vec::Vec},
    proc_macro_lib::ktest,
};

//pub mod reverse_scan;
//pub mod naive;
//pub mod solid_state;
pub mod regalloc_ng;

pub trait RegisterAllocator {
    // A is for the generic memory allocator, NOT anything to do with the register
    // allocator
    fn allocate<A: MemAlloc>(&mut self, instructions: &mut Vec<Instruction<A>, A>);
}
