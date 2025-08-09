use {
    crate::rudder::{analysis::dfa::StatementUseAnalysis, opt::OptimizationContext},
    common::{
        arena::{Arena, Ref},
        rudder::{block::Block, function::Function},
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
    let mut _changed = false;

    let mut _sua = StatementUseAnalysis::new(arena, b, &ctx.purity);

    // for stmt in b
    //     .get(sua.block_arena())
    //     .statements()
    //     .iter()
    //     .copied()
    //     .collect::<Vec<_>>()
    // {
    //     match stmt.get(b.get(sua.block_arena()).arena()).clone() {
    //         Statement::Cast { typ, value, .. } => {
    //             // If the cast is to the same type then it is probably useless
    //             if value
    //                 .get(b.get(sua.block_arena()).arena())
    //                 .clone()
    //                 .typ(b.get(sua.block_arena()).arena())
    //                 .as_ref()
    //                 == Some(&typ)
    //             {
    //                 // replace uses
    //                 if let Some(uses) = sua.get_uses(stmt) {
    //                     uses.iter().for_each(|s| {
    //                         s.get_mut(b.get(sua.block_arena()).arena_mut())
    //                             .replace_use(stmt, value);
    //                     });
    //                 }

    //                 changed = true;
    //             }
    //         }
    //         _ => {}
    //     }
    // }

    _changed
}
