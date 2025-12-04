// Although some builtin fns are handled in rudder, we handle some basic ones
// here to make the destruct_composites pass

use {
    crate::{
        DELETED_CALLS,
        boom::{
            Ast, Bit, Expression, Literal, Operation, Size, Statement, Type, Value, passes::Pass,
        },
    },
    common::{
        hashmap::{HashMap, HashSet},
        intern::InternedString,
    },
    core::panic,
    num_bigint::BigInt,
    once_cell::sync::Lazy,
    rayon::iter::{IntoParallelRefIterator, ParallelIterator},
    regex::Regex,
    sailrs::shared::Shared,
};

const EQ_ANY_GENERIC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^eq_any<([0-9a-zA-Z_%<>]+)>$").unwrap());

const VECTOR_ACCESS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^plain_vector_access<([0-9a-zA-Z_%<>]+)>$").unwrap());

const VECTOR_UPDATE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^plain_vector_update<([0-9a-zA-Z_%<>]+)>|internal_vector_update$").unwrap()
});

#[derive(Debug, Default)]
pub struct HandleBuiltinFunctions;

impl HandleBuiltinFunctions {
    /// Create a new Pass object
    pub fn new_boxed() -> Box<dyn Pass> {
        Box::<Self>::default()
    }
}

impl Pass for HandleBuiltinFunctions {
    fn name(&self) -> &'static str {
        "HandleBuiltinFunctions"
    }

    fn reset(&mut self) {}

    fn run(&mut self, ast: Shared<Ast>) -> bool {
        let undefined_enum_constructors = ast
            .get()
            .enums
            .keys()
            .map(|name| format!("undefined_{name}").into())
            .collect::<HashSet<InternedString>>();

        ast.get().functions.par_iter().for_each(|(_, def)| {
            def.entry_block.iter().for_each(|b| {
                let mut local_vectors = HashMap::default();

                b.set_statements(
                    b.statements()
                        .into_iter()
                        .map(|s| {
                            if let Statement::VariableDeclaration { name, typ } = &*(s.get()) {
                                if let Type::FixedVector {
                                    length,
                                    element_type,
                                } = &*typ.get()
                                {
                                    local_vectors.insert(*name, (*length, element_type.clone()));
                                }
                            }

                            if let Statement::FunctionCall {
                                name,
                                expression: Some(expression),
                                arguments,
                            } = &*(s.get())
                            {
                                // replace all eq functions with an equal operation
                                if EQ_ANY_GENERIC.is_match(name.as_ref())
                                    || name.as_ref() == "eq_int"
                                    || name.as_ref() == "eq_bits"
                                {
                                    assert_eq!(2, arguments.len());
                                    op(
                                        expression,
                                        Operation::Equal(
                                            arguments[0].clone(),
                                            arguments[1].clone(),
                                        ),
                                    )
                                } else if name.as_ref() == "IsZero" {
                                    op(
                                        expression,
                                        Operation::Equal(
                                            arguments[0].clone(),
                                            Shared::new(Value::Literal(Shared::new(Literal::Int(
                                                0.into(),
                                            )))),
                                        ),
                                    )
                                } else if VECTOR_ACCESS.is_match(name.as_ref()) {
                                    Shared::new(Statement::Copy {
                                        expression: expression.clone(),
                                        value: Shared::new(Value::VectorAccess {
                                            value: arguments[0].clone(),
                                            index: arguments[1].clone(),
                                        }),
                                    })
                                } else if VECTOR_UPDATE.is_match(name.as_ref()) {
                                    Shared::new(Statement::Copy {
                                        expression: expression.clone(),
                                        value: Shared::new(Value::VectorMutate {
                                            vector: arguments[0].clone(),
                                            element: arguments[2].clone(),
                                            index: arguments[1].clone(),
                                        }),
                                    })
                                } else if undefined_enum_constructors.contains(name) {
                                    Shared::new(Statement::Copy {
                                        expression: expression.clone(),
                                        value: Shared::new(Value::Literal(Shared::new(
                                            Literal::Int(0.into()),
                                        ))),
                                    })
                                } else if name.as_ref() == "internal_vector_init" {
                                    let Value::Literal(lit) = &*arguments[0].get() else {
                                        panic!()
                                    };

                                    let Literal::Int(n) = &*lit.get() else {
                                        panic!()
                                    };

                                    let n = usize::try_from(n).unwrap();

                                    let Expression::Identifier(local) = expression else {
                                        panic!()
                                    };

                                    let (length, element_type) = local_vectors.get(local).unwrap();

                                    assert_eq!(usize::try_from(*length).unwrap(), n);

                                    let element = match &*element_type.get() {
                                        Type::Bits {
                                            size: Size::Static(width),
                                        } => Shared::new(Literal::Bits(vec![Bit::Zero; *width])),
                                        Type::Integer {
                                            size: Size::Unknown,
                                        } => Shared::new(Literal::Int(BigInt::ZERO)),
                                        Type::Bool => Shared::new(Literal::Bit(Bit::Zero)),
                                        t => todo!("{t:?}"),
                                    };

                                    Shared::new(Statement::Copy {
                                        expression: expression.clone(),
                                        value: Shared::new(Value::Literal(Shared::new(
                                            Literal::Vector(vec![element; n]),
                                        ))),
                                    })
                                } else if DELETED_CALLS.contains(&name.as_ref()) {
                                    Shared::new(Statement::Copy {
                                        expression: expression.clone(),
                                        value: Shared::new(Value::Literal(Shared::new(
                                            Literal::Unit,
                                        ))),
                                    })
                                } else if name.as_ref() == "undefined_bool" {
                                    Shared::new(Statement::Copy {
                                        expression: expression.clone(),
                                        value: Shared::new(Value::Literal(Shared::new(
                                            Literal::Bool(false),
                                        ))),
                                    })
                                } else if name.as_ref() == "undefined_int" {
                                    Shared::new(Statement::Copy {
                                        expression: expression.clone(),
                                        value: Shared::new(Value::Literal(Shared::new(
                                            Literal::Int(0.into()),
                                        ))),
                                    })
                                } else if name.as_ref() == "undefined_real" {
                                    Shared::new(Statement::Copy {
                                        expression: expression.clone(),
                                        value: Shared::new(Value::Real {
                                            numerator: Shared::new(Value::Literal(Shared::new(
                                                Literal::Int(0.into()),
                                            ))),
                                            denominator: Shared::new(Value::Literal(Shared::new(
                                                Literal::Int(1.into()),
                                            ))),
                                        }),
                                    })
                                } else {
                                    s.clone()
                                }
                            } else {
                                s.clone()
                            }
                        })
                        .collect(),
                );
            });
        });

        ast.get_mut()
            .functions
            .retain(|name, _| !undefined_enum_constructors.contains(name));

        false
    }
}

fn op(expression: &Expression, op: Operation) -> Shared<Statement> {
    Shared::new(Statement::Copy {
        expression: expression.clone(),
        value: Shared::new(Value::Operation(op)),
    })
}
