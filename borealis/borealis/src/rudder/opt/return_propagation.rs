use {
    crate::rudder::{
        Function, StatementKind, analysis::cfg::ControlFlowGraphAnalysis, opt::OptimizationContext,
    },
    log::trace,
};

pub fn run(_ctx: &OptimizationContext, f: Function) -> bool {
    let cfg = ControlFlowGraphAnalysis::new(&f);

    // If a block contains only a return statement, replace call sites that jump
    // (not branch) to it

    let mut changed = false;
    for block in f.block_iter().map(|b| b.get(f.block_arena())) {
        // If the block has exactly one statement
        if block.statements().len() == 1 {
            let Some(terminator) = block.terminator_statement() else {
                continue;
            };

            match terminator.kind() {
                StatementKind::Return { value } => {
                    trace!("found single statement return block");

                    // for each predecessor, if they end in a "jump"
                    for pred in cfg.predecessors_for(&block).unwrap() {
                        let pred_terminator = pred.terminator_statement().unwrap();

                        if let StatementKind::Jump { target } = pred_terminator.kind() {
                            if target != block {
                                panic!("cf mismatch");
                            }

                            trace!("replacing jump with return");
                            pred_terminator.replace_kind(StatementKind::Return { value: None });
                            changed = true;
                        }

                        // if they end in a branch, try to coalesce to a single
                        // return block.
                    }
                }
                _ => {
                    // nothing to do for this terminator
                }
            }
        }
    }

    changed
}
