use {
    crate::rudder::opt::OptimizationContext,
    common::{
        arena::{Arena, Ref},
        rudder::{block::Block, function::Function},
    },
};

pub fn run(_ctx: &OptimizationContext, f: &mut Function) -> bool {
    let mut changed = false;

    for block in f.block_iter().collect::<Vec<_>>().into_iter() {
        changed |= run_on_block(f.arena_mut(), block);
    }

    changed
}

fn run_on_block(arena: &mut Arena<Block>, b: Ref<Block>) -> bool {
    // If this block jumps to a block that only panics, then:
    // Either:
    // (1) If this is a jump, inline the panic (although this might already be done)
    // (2) If this is a branch, insert an "assert" on the branch condition (think
    // about direction)     and rewrite the branch to be a jump to the
    // alternative path

    if let Some(terminator) = b.get(arena).terminator_statement() {
        match terminator.get(b.get(arena).arena()) {
            common::rudder::statement::Statement::Branch { .. } => {
                // Check target block
            }

            _ => {}
        }
    }

    false
}
