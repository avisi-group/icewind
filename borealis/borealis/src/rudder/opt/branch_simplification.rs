use {
    crate::rudder::opt::OptimizationContext,
    common::{
        arena::{Arena, Ref},
        rudder::{
            block::Block,
            function::Function,
            statement::{Statement, UnaryOperationKind},
        },
    },
    itertools::Itertools,
    log::trace,
};

pub fn run(_ctx: &OptimizationContext, f: &mut Function) -> bool {
    // check condition for branch.  if it's const, replace with a jump.  if both
    // targets are the same, replace with a jump

    let mut changed = false;
    for block_ref in f.block_iter().collect::<Vec<_>>() {
        let block = block_ref.get(f.arena());
        let Some(terminator_ref) = block.terminator_statement() else {
            continue;
        };

        let Statement::Branch {
            condition,
            true_target,
            false_target,
        } = terminator_ref.get(block.arena()).clone()
        else {
            continue;
        };

        let condition = condition.get(block.arena()).clone();

        if let Statement::Constant(value) = condition {
            trace!("found constant branch statement {}", value);

            let block = block_ref.get_mut(f.arena_mut());

            if value.is_zero() == Some(true) {
                terminator_ref
                    .get_mut(block.arena_mut())
                    .replace(Statement::Jump {
                        target: false_target,
                    });
            } else {
                terminator_ref
                    .get_mut(block.arena_mut())
                    .replace(Statement::Jump {
                        target: true_target,
                    });
            }

            changed = true;
        } else if let Statement::UnaryOperation { kind, value } = condition {
            match kind {
                UnaryOperationKind::Not => {
                    let new_true = false_target;
                    let new_false = true_target;

                    let block = block_ref.get_mut(f.arena_mut());

                    terminator_ref
                        .get_mut(block.arena_mut())
                        .replace(Statement::Branch {
                            condition: value,
                            true_target: new_true,
                            false_target: new_false,
                        });

                    changed = true;
                }
                _ => {}
            }
        } else if true_target == false_target
            || equivalent_blocks(f.arena(), true_target, false_target)
        {
            let block = block_ref.get_mut(f.arena_mut());

            terminator_ref
                .get_mut(block.arena_mut())
                .replace(Statement::Jump {
                    target: true_target,
                }); // todo: verify this will be inlined/threaded if needed?
            changed = true;
        }
    }

    changed
}

/// Returns whether two blocks contain the same statements
///
/// Todo: make this smarter and detect
fn equivalent_blocks(arena: &Arena<Block>, a: Ref<Block>, b: Ref<Block>) -> bool {
    let block_a = a.get(arena);
    let block_b = b.get(arena);

    !block_a
        .statements()
        .iter()
        // panic if not equal length
        .zip_eq(block_b.statements().iter())
        // if any statements are not equal, short circuits returning true, which we then `not` at
        // the beginning of the iterator
        .any(|(ref_a, ref_b)| {
            let statement_a = ref_a.get(block_a.arena());
            let statement_b = ref_b.get(block_b.arena());

            statement_a.to_string(block_a.arena()) != statement_b.to_string(block_b.arena())
        })
}
