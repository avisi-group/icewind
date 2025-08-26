use {
    crate::rudder::{analysis::dfa::StatementUseAnalysis, opt::OptimizationContext},
    common::{
        arena::{Arena, Ref},
        rudder::{
            block::Block,
            function::Function,
            statement::{CastOperationKind, Statement},
            types::Type,
        },
    },
};

pub fn run(ctx: &OptimizationContext, f: &mut Function) -> bool {
    let mut changed = false;

    for block in f.block_iter().collect::<Vec<_>>().into_iter() {
        changed |= run_on_block(ctx, f.arena_mut(), block);
    }

    changed
}

fn run_on_block(ctx: &OptimizationContext, arena: &mut Arena<Block>, b: Ref<Block>) -> bool {
    let mut sua = StatementUseAnalysis::new(arena, b, &ctx.purity);

    for stmt in b
        .get(sua.block_arena())
        .statements()
        .iter()
        .copied()
        .collect::<Vec<_>>()
    {
        match stmt.get(b.get(sua.block_arena()).arena()).clone() {
            Statement::Cast {
                kind: CastOperationKind::Convert,
                typ,
                value,
            } => {
                let source_type = value
                    .get(b.get(sua.block_arena()).arena())
                    .clone()
                    .typ(b.get(sua.block_arena()).arena())
                    .clone()
                    .unwrap();

                // no-op, remove
                if source_type == typ {
                    // remove the cast
                    if let Some(uses) = sua.get_uses(stmt).cloned() {
                        uses.iter().for_each(|s| {
                            s.get_mut(b.get_mut(sua.block_arena()).arena_mut())
                                .replace_use(stmt, value);
                        });

                        // need to recompute SUA
                        return true;
                    }
                }

                // if we're casting to a vector of length 0
                if let Type::Vector {
                    element_count: 0,
                    element_type: target_element_type,
                } = typ
                {
                    // from a vector
                    if let Type::Vector {
                        element_count,
                        element_type: source_element_type,
                    } = source_type
                    {
                        // with a >0 element count
                        // and the types are the same
                        if element_count > 0 && *target_element_type == *source_element_type {
                            // remove the cast
                            if let Some(uses) = sua.get_uses(stmt).cloned() {
                                uses.iter().for_each(|s| {
                                    s.get_mut(b.get_mut(sua.block_arena()).arena_mut())
                                        .replace_use(stmt, value);
                                });

                                // need to recompute SUA
                                return true;
                            }
                        }
                    }
                }
            }
            Statement::Cast {
                kind: CastOperationKind::Reinterpret,
                typ,
                value,
            } => {
                let arena = b.get_mut(sua.block_arena()).arena_mut();
                if Some(typ) == value.get(arena).typ(arena) {
                    let value_cloned = value.get(arena).clone();
                    stmt.get_mut(arena).replace(value_cloned);
                }
            }

            // too powerful
            // but for real, something breaks if you add a pass that removes all casts if src type
            // == dst type probably zero/sign extension stuff?
            // todo
            // Statement::Cast {
            //     kind: _,
            //     typ,
            //     value,
            // } => {
            //     let arena = b.get_mut(sua.block_arena()).arena_mut();
            //     if Some(typ) == value.get(arena).typ(arena) {
            //         stmt.get_mut(arena).replace_use(stmt, value);
            //     }
            // }
            _ => {}
        }
    }

    false
}
