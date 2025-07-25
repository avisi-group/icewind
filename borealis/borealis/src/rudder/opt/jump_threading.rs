use {
    crate::rudder::opt::OptimizationContext,
    common::{
        arena::Ref,
        rudder::{block::Block, function::Function, statement::Statement},
    },
};

pub fn run(_ctx: &OptimizationContext, f: &mut Function) -> bool {
    let mut changed = false;

    for block in f.block_iter().collect::<Vec<_>>().into_iter() {
        changed |= run_on_block(f, block);
    }

    changed
}

fn target_for_threadable(f: &Function, block_ref: Ref<Block>) -> Option<Ref<Block>> {
    let block = block_ref.get(f.arena());
    if block.len() == 1 {
        if let Statement::Jump { target } = block.terminator_statement().unwrap().get(block.arena())
        {
            Some(*target)
        } else {
            None
        }
    } else {
        None
    }
}

fn run_on_block(f: &mut Function, block_ref: Ref<Block>) -> bool {
    let block = block_ref.get(f.arena());
    let terminator_ref = block.terminator_statement().unwrap();

    match terminator_ref.get(block.arena()).clone() {
        Statement::Jump { target } => {
            if let Some(thread_to) = target_for_threadable(f, target) {
                terminator_ref
                    .get_mut(block_ref.get_mut(f.arena_mut()).arena_mut())
                    .replace(Statement::Jump { target: thread_to });
                true
            } else {
                false
            }
        }
        Statement::Branch {
            condition,
            true_target,
            false_target,
        } => {
            let mut changed = false;

            let true_target = if let Some(true_thread_to) = target_for_threadable(f, true_target) {
                changed = true;
                true_thread_to
            } else {
                true_target
            };

            let false_target = if let Some(false_thread_to) = target_for_threadable(f, false_target)
            {
                changed = true;
                false_thread_to
            } else {
                false_target
            };

            if changed {
                terminator_ref
                    .get_mut(block_ref.get_mut(f.arena_mut()).arena_mut())
                    .replace(Statement::Branch {
                        condition,
                        true_target,
                        false_target,
                    });
            }

            changed
        }
        _ => false,
    }
}
