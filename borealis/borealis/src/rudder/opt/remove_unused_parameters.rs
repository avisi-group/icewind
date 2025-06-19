use {
    crate::rudder::{analysis, opt::OptimizationContext},
    common::rudder::{Model, block::Block},
    parking_lot::Mutex,
    rayon::prelude::*,
    std::sync::atomic::{AtomicBool, Ordering},
};

pub fn run(_ctx: &OptimizationContext, model: &mut Model) -> bool {
    let changed = AtomicBool::new(false);

    let dead_parameters = Mutex::new(vec![]);

    model.functions_mut().par_values_mut().for_each(|function| {
        let dfa = analysis::dfa::SymbolUseAnalysis::new(function);

        for (i, sym) in function.parameters().iter().enumerate().rev() {
            // rev because we want to remove parameters in reverse order to not mess up the
            // indices
            if dfa.is_symbol_dead(&sym) {
                dead_parameters.lock().push((function.name(), i));
                function.remove_parameter(&sym);
            }
        }
    });

    let dead_parameters = dead_parameters.into_inner();

    //     fix call sites
    model.functions_mut().par_values_mut().for_each(|function| {
        function
            .block_iter()
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|b| {
                let Block {
                    statements,
                    statement_arena,
                } = b.get_mut(function.arena_mut());

                for s in statements {
                    if let common::rudder::statement::Statement::Call { target, args, .. } =
                        s.get_mut(statement_arena)
                    {
                        dead_parameters
                            .iter()
                            .filter(|(name, _)| *name == *target)
                            .for_each(|(_, index)| {
                                args.remove(*index);
                                changed.store(true, Ordering::Relaxed);
                            });
                    }
                }
            });
    });

    changed.load(Ordering::Relaxed)
}
