use {
    crate::rudder::analysis::pure::PurityAnalysis,
    common::rudder::{Model, function::Function},
    log::{debug, trace},
    rayon::iter::{IntoParallelRefMutIterator, ParallelIterator},
};

mod bit_simplification;
mod block_inliner;
mod branch_simplification;
mod constant_folding;
mod constant_propagation;
mod dead_stmt_elimination;
mod dead_symbol_elimination;
mod dead_write_elimination;
mod jump_threading;
mod phi_analysis;
//mod return_propagation;
//mod tail_calls;
mod function_inliner;
mod local_tuple_removal;
mod panic_asserter;
mod remove_unused_parameters;
mod useless_cast_elimination;
mod variable_elimination;
mod vector_folding;

pub enum OptLevel {
    Level3,
}

type FunctionPassFn = fn(&OptimizationContext, &mut Function) -> bool;
type FunctionPass = (&'static str, FunctionPassFn);

type ModelPassFn = fn(&OptimizationContext, &mut Model) -> bool;
type ModelPass = (&'static str, ModelPassFn);

// DO NOT OPTIMIZE AWAY
pub const INTRINSICS: &[&'static str] = &["sail_tlbi"];

// --- MODEL PASSES --- //
static FUNCTION_PASS_RUNNER: ModelPass = ("function-pass-runner", run_function_passes);
static REMOVE_UNUSED_PARAMETERS: ModelPass = (
    "remove-unused-function-parameters",
    remove_unused_parameters::run,
);
static FUNCTION_INLINER: ModelPass = ("function-inliner", function_inliner::run);

// --- FUNCTION PASSES --- //

static BLOCK_INLINER: FunctionPass = ("block-inliner", block_inliner::run);
static JUMP_THREADING: FunctionPass = ("jump-threading", jump_threading::run);
static VARIABLE_ELIMINATION: FunctionPass = ("var-elimination", variable_elimination::run);
static DEAD_SYMBOL_ELIMINATION: FunctionPass =
    ("dead-symbol-elimination", dead_symbol_elimination::run);
static DEAD_WRITE_ELIMINATION: FunctionPass =
    ("dead-write-elimination", dead_write_elimination::run);
static DEAD_STMT_ELIMINATION: FunctionPass = ("dead-stmt-elimination", dead_stmt_elimination::run);
static CONSTANT_PROPAGATION: FunctionPass = ("constant-propagation", constant_propagation::run);
static CONSTANT_FOLDING: FunctionPass = ("constant-folding", constant_folding::run);
static BIT_SIMPLIFICATION: FunctionPass = ("bit-simplification", bit_simplification::run);
//static RETURN_PROPAGATION: FunctionPass = ("return-propagation",
// return_propagation::run);
static BRANCH_SIMPLIFICATION: FunctionPass = ("branch-simplification", branch_simplification::run);
static PHI_ANALYSIS: FunctionPass = ("phi-analysis", phi_analysis::run);
// static TAIL_CALL: FunctionPass = ("tail-call", tail_calls::run);
static VECTOR_FOLDING: FunctionPass = ("vector-folding", vector_folding::run);
static LOCAL_TUPLE_REMOVAL: FunctionPass = ("local-tuple-removal", local_tuple_removal::run);
static PANIC_ASSERTER: FunctionPass = ("panic-asserter", panic_asserter::run);
static USELESS_CAST_ELIMINATION: FunctionPass =
    ("useless-cast-elimination", useless_cast_elimination::run);
// static DESTROY_BITVECTORS: FunctionPass = ("destroy-bitvectors",
// destroy_bitvectors::run); static MATERIALISE_APINTS: FunctionPass =
// ("materialise-apints", materialise_apints::run);

struct OptimizationContext {
    optimization_level: OptLevel,
    purity: PurityAnalysis,
}

fn run_function_passes(ctx: &OptimizationContext, model: &mut Model) -> bool {
    let passes: Vec<FunctionPass> = match ctx.optimization_level {
        OptLevel::Level3 => vec![
            BLOCK_INLINER,
            JUMP_THREADING,
            BRANCH_SIMPLIFICATION,
            PANIC_ASSERTER,
            DEAD_SYMBOL_ELIMINATION,
            DEAD_WRITE_ELIMINATION,
            DEAD_STMT_ELIMINATION,
            USELESS_CAST_ELIMINATION,
            VARIABLE_ELIMINATION,
            BIT_SIMPLIFICATION,
            CONSTANT_PROPAGATION,
            CONSTANT_FOLDING,
            LOCAL_TUPLE_REMOVAL,
            VECTOR_FOLDING,
            PHI_ANALYSIS,
        ],
    };

    model
        .functions_mut()
        .par_iter_mut()
        .for_each(|(name, function)| {
            let mut changed = true;

            debug!("optimising function {name:?}");

            while changed {
                changed = false;
                for pass in &passes {
                    debug!("running function pass {}", pass.0);

                    while pass.1(ctx, function) {
                        changed = true;
                    }
                }
            }
        });

    false
}

pub fn optimise(model: &mut Model, optimization_level: OptLevel) {
    let purity = PurityAnalysis::new(&model);
    let context = OptimizationContext {
        optimization_level,
        purity,
    };

    let passes: Vec<ModelPass> = vec![
        FUNCTION_PASS_RUNNER,
        REMOVE_UNUSED_PARAMETERS,
        FUNCTION_INLINER,
    ];

    let mut changed = true;

    while changed {
        changed = false;

        for pass in &passes {
            trace!("running model pass {}", pass.0);

            while pass.1(&context, model) {
                changed = true;
            }

            // pass.1(&context, model);
        }
    }
}
