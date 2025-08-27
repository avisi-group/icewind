//! Monomorphize vectors (not bitvectors)
//!
//! If a register is copied to a local var, and the register has a known length,
//! change the local var to also be known length
//!
//! Not a great heuristic, possible bugs if there are multiple copies, or ???

use {
    crate::boom::{
        Ast, Expression, Statement, Type, Value,
        control_flow::ControlFlowBlock,
        passes::{Pass, any::AnyExt},
    },
    common::hashmap::HashMap,
    sailrs::shared::Shared,
};

#[derive(Debug, Default)]
pub struct MonomorphizeVectors;

impl MonomorphizeVectors {
    /// Create a new Pass object
    pub fn new_boxed() -> Box<dyn Pass> {
        Box::<Self>::default()
    }
}

impl Pass for MonomorphizeVectors {
    fn name(&self) -> &'static str {
        "MonomorphizeVectors"
    }

    fn reset(&mut self) {}

    fn run(&mut self, ast: Shared<Ast>) -> bool {
        ast.get()
            .functions
            .values()
            .map(|def| monomorphize_vectors(ast.clone(), def.entry_block.clone()))
            .any()
    }
}

fn monomorphize_vectors(ast: Shared<Ast>, entry_block: ControlFlowBlock) -> bool {
    let mut did_change = false;

    let mut fixed_vectors = ast
        .get()
        .registers
        .iter()
        .filter(|(_, typ)| matches!(&*typ.get(), Type::FixedVector { .. }))
        .map(|(name, typ)| (*name, typ.get().clone()))
        .collect::<HashMap<_, _>>();

    let mut local_dynamic_vectors = HashMap::default();

    for s in entry_block.iter().flat_map(|b| b.statements()) {
        match &*s.get() {
            Statement::VariableDeclaration { name, typ } => match &*typ.get() {
                Type::FixedVector { .. } => {
                    fixed_vectors.insert(*name, typ.get().clone());
                }
                Type::Vector { .. } => {
                    local_dynamic_vectors.insert(*name, s.clone());
                }
                _ => (),
            },

            // only consider copies into identifiers
            Statement::Copy {
                expression: Expression::Identifier(destination),
                value,
            } => {
                // if the source is an identifier or a vector mutate
                let source = match &*value.get() {
                    Value::Identifier(source) => *source,
                    Value::VectorMutate { vector, .. } => {
                        let Value::Identifier(source) = &*vector.get() else {
                            continue;
                        };
                        *source
                    }
                    _ => continue,
                };

                // and the source is a fixed vector
                let Some(source_type) = fixed_vectors.get(&source) else {
                    continue;
                };

                // and if the destination is a dynamic vector
                let Some(destination_decl) = local_dynamic_vectors.get(destination) else {
                    continue;
                };

                // replace the destination declaration type with the more concrete source type
                let Statement::VariableDeclaration { typ, .. } = &mut *destination_decl.get_mut()
                else {
                    panic!()
                };

                log::debug!(
                    "in copy from {source:?} to {destination:?}, we are changing the type of {destination:?} from {typ:?} to {source_type:?}"
                );
                *typ = Shared::new(source_type.clone());

                did_change = true;
            }
            _ => {}
        }
    }

    // look for copies into vectors of unknown length
    // change type declarations

    did_change
}
