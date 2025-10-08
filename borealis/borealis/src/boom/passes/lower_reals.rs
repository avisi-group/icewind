use {
    crate::boom::{
        Ast, FunctionDefinition, NamedType, Parameter, Size, Statement, Type, Value,
        control_flow::{ControlFlowBlock, Terminator},
        passes::Pass,
    },
    common::intern::InternedString,
    rayon::iter::ParallelIterator,
    sailrs::shared::Shared,
};

#[derive(Debug, Default)]
pub struct LowerReals;

impl LowerReals {
    /// Create a new Pass object
    pub fn new_boxed() -> Box<dyn Pass> {
        Box::<Self>::default()
    }
}

impl Pass for LowerReals {
    fn name(&self) -> &'static str {
        "LowerReals"
    }

    fn reset(&mut self) {}

    fn run(&mut self, ast: Shared<Ast>) -> bool {
        // implement body of "to_real(p0: %i)" as `return (p0, 1)`
        ast.get_mut()
            .functions
            .get_mut(&InternedString::from_static("to_real"))
            .unwrap()
            .entry_block = {
            let block = ControlFlowBlock::new();
            block.set_terminator(Terminator::Return(Some(Value::Tuple(vec![
                Shared::new(Value::Identifier("p0".into())),
                Shared::new(Value::Literal(Shared::new(crate::boom::Literal::Int(
                    1.into(),
                )))),
            ]))));
            block
        };

        // replace all real types with (i, i)
        ast.get_mut().registers.values().for_each(try_replace_type);
        ast.get_mut().structs.iter().for_each(|(_, fields)| {
            fields
                .iter()
                .for_each(|NamedType { typ, .. }| try_replace_type(typ));
        });
        ast.get_mut().functions.par_values().for_each(
            |FunctionDefinition {
                 signature,
                 entry_block,
             }| {
                if let Some(ret) = &signature.return_type {
                    try_replace_type(&ret);
                }

                signature
                    .parameters
                    .get()
                    .iter()
                    .for_each(|Parameter { typ, .. }| try_replace_type(&typ));

                entry_block
                    .iter()
                    .flat_map(|block| block.statements())
                    .for_each(|s| {
                        if let Statement::VariableDeclaration { typ, .. } = &*s.get() {
                            try_replace_type(&typ);
                        }
                    });
            },
        );

        false
    }
}

fn try_replace_type(typ: &Shared<Type>) {
    let mut typ = typ.get_mut();
    match &*typ {
        Type::Real => {
            *typ = Type::Tuple(vec![
                Shared::new(Type::Integer {
                    size: Size::Unknown,
                }),
                Shared::new(Type::Integer {
                    size: Size::Unknown,
                }),
            ])
        }

        Type::Struct { fields, .. } => fields
            .iter()
            .for_each(|NamedType { typ, .. }| try_replace_type(typ)),
        Type::Tuple(vec) => vec.iter().for_each(try_replace_type),

        Type::Vector { element_type } | Type::FixedVector { element_type, .. } => {
            try_replace_type(element_type)
        }

        _ => (),
    }
}
