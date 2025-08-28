use {
    crate::boom::{
        Ast, Expression, FunctionDefinition, Literal, NamedType, Operation, Parameter, Size,
        Statement, Type, Value, Visitor, control_flow::Terminator, passes::Pass, visitor::Walkable,
    },
    common::{hashmap::HashMap, intern::InternedString},
    itertools::Itertools,
    rayon::iter::{IntoParallelRefIterator, ParallelIterator},
    sailrs::shared::Shared,
    std::fmt::{self, Display},
};

#[derive(Debug, Default)]
pub struct DestructComposites;

impl DestructComposites {
    /// Create a new Pass object
    pub fn new_boxed() -> Box<dyn Pass> {
        Box::<Self>::default()
    }
}

impl Pass for DestructComposites {
    fn name(&self) -> &'static str {
        "DestructComposites"
    }

    fn reset(&mut self) {}

    fn run(&mut self, ast: Shared<Ast>) -> bool {
        let composites = {
            let mut map = HashMap::default();
            map.extend(ast.get().unions.iter().map(|(name, fields)| {
                (
                    *name,
                    Shared::new(Type::Union {
                        name: *name,
                        fields: fields.clone(),
                    }),
                )
            }));
            map.extend(ast.get().structs.iter().map(|(name, fields)| {
                (
                    *name,
                    Shared::new(Type::Struct {
                        name: *name,
                        fields: fields.clone(),
                    }),
                )
            }));
            map
        };

        let destructed_registers = destruct_registers(ast.clone());

        // function signatures need fixing first, then storing for lookup when handling
        // function calls
        let destructed_return_type_by_function = destruct_function_return_types(ast.clone());
        let destructed_parameters_by_function = destruct_function_parameters(ast.clone());

        ast.get().functions.par_iter().for_each(|(name, def)| {
            destruct_locals(
                *name,
                &composites,
                &destructed_registers,
                &destructed_return_type_by_function,
                &destructed_parameters_by_function,
                def,
            );
        });

        false
    }
}

/// Either a destination or a source location
#[derive(Debug, Clone, PartialEq, Eq)]
enum DataLocation {
    Identifier(InternedString),
    Fields {
        root: InternedString,
        fields: Vec<InternedString>,
    },
}

impl Display for DataLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier(ident) => write!(f, "{ident}"),
            Self::Fields { root, fields } => {
                write!(f, "{root}.{}", fields.iter().join("."))
            }
        }
    }
}

impl DataLocation {
    pub fn root(&self) -> InternedString {
        match self {
            DataLocation::Identifier(root) | DataLocation::Fields { root, .. } => *root,
        }
    }

    pub fn to_ident(&self) -> InternedString {
        match self {
            DataLocation::Identifier(ident) => *ident,
            DataLocation::Fields { root, fields } => {
                format!("{root}_{}", fields.iter().join("_")).into()
            }
        }
    }

    pub fn try_from(value: Shared<Value>) -> Option<Self> {
        fn build_fields(value: Shared<Value>) -> Option<(InternedString, Vec<InternedString>)> {
            let mut fields = vec![];

            let mut current_value = value.get().clone();
            loop {
                match current_value {
                    Value::Identifier(root) => {
                        fields.reverse();
                        return Some((root, fields));
                    }
                    Value::Field { value, field_name } => {
                        fields.push(field_name);
                        current_value = value.get().clone();
                    }
                    Value::Struct { fields: f, .. } => {
                        let Some(last) = fields.last() else {
                            // encountered struct with no field accesses, bailing
                            return None;
                        };

                        let nv = f
                            .iter()
                            .find(|nv| nv.name == *last)
                            .expect("tried to access field on struct that does not exist");

                        // go back up one layer to cancel out
                        fields.pop();
                        current_value = nv.value.get().clone();
                    }

                    _ => return None,
                }
            }
        }

        build_fields(value.clone()).map(|(root, fields)| {
            if fields.is_empty() {
                Self::Identifier(root)
            } else {
                Self::Fields { root, fields }
            }
        })
    }
}

impl From<&Expression> for DataLocation {
    fn from(value: &Expression) -> Self {
        fn build_fields(expr: &Expression) -> (InternedString, Vec<InternedString>) {
            let mut fields = vec![];

            let mut current_expr = expr.clone();
            loop {
                match current_expr {
                    Expression::Identifier(root) => {
                        fields.reverse();
                        return (root, fields);
                    }
                    Expression::Field { expression, field } => {
                        fields.push(field);
                        current_expr = *expression;
                    }
                    _ => panic!(),
                }
            }
        }

        match value {
            Expression::Identifier(dst) => Self::Identifier(*dst),
            Expression::Field { .. } => {
                let (root, fields) = build_fields(value);
                Self::Fields { root, fields }
            }
            _ => panic!(),
        }
    }
}

/// split locally declared unions into a tag and a local variable the size of
/// the largest value?
fn destruct_locals(
    function_name: InternedString,
    composites: &HashMap<InternedString, Shared<Type>>,
    destructed_registers: &HashMap<InternedString, Shared<Type>>,
    destructed_return_types_by_function: &HashMap<InternedString, Shared<Type>>,
    destructed_parameters_by_function: &HashMap<
        InternedString,
        HashMap<InternedString, Shared<Type>>,
    >,
    def: &FunctionDefinition,
) {
    let mut destructed_local_variables = destructed_parameters_by_function
        .get(&function_name)
        .cloned()
        .unwrap_or_default();

    def.entry_block.iter().for_each(|block| {
        let destructed = block
            .statements()
            .into_iter()
            .flat_map(|statement| {
                let clone = statement.clone();

                match &*statement.get() {
                    Statement::VariableDeclaration {
                        name: variable_name,
                        typ,
                    } => {
                        // if we have a type declaration for a union, emit value and tag variables
                        // instead
                        if is_type_composite(typ.clone()) {
                            destructed_local_variables.insert(*variable_name, typ.clone());
                            destruct_variable(*variable_name, typ.clone())
                                .into_iter()
                                .map(|(name, typ)| Statement::VariableDeclaration { name, typ })
                                .map(Shared::new)
                                .collect()
                        } else {
                            return vec![clone];
                        }
                    }
                    Statement::Copy { expression, value } => {
                        let destination = DataLocation::from(expression);

                        // if source is not a location, it won't be a struct identifier or field
                        // access so return early
                        let (source, vector_access) = match DataLocation::try_from(value.clone()) {
                            Some(source) => (source, None),
                            None => match &*value.get() {
                                Value::VectorAccess { value, index } => (
                                    DataLocation::try_from(value.clone()).unwrap(),
                                    Some(index.clone()),
                                ),
                                Value::VectorMutate {
                                    vector,
                                    element,
                                    index,
                                } => {
                                    let Value::Identifier(vector) = &*vector.get() else {
                                        todo!()
                                    };
                                    let Value::Identifier(element) = &*element.get() else {
                                        // if the element is a literal or something bail out
                                        return vec![Shared::new(Statement::Copy {
                                            expression: Expression::Identifier(
                                                destination.to_ident(),
                                            ),
                                            value: value.clone(),
                                        })];
                                    };

                                    // was element destructed?
                                    // assumes element and vector have same type (this is
                                    // reasonable)
                                    if let Some(typ) = destructed_local_variables
                                        .get(element)
                                        .or_else(|| destructed_registers.get(element))
                                    {
                                        // foo = (baz[0] = bar)
                                        // =>
                                        // foo_a = (baz_a[0] = bar_a)
                                        // foo_b = (baz_b[0] = bar_b)
                                        return destruct_variable(*element, typ.clone())
                                            .into_iter()
                                            .zip(destruct_variable(
                                                destination.to_ident(),
                                                typ.clone(),
                                            ))
                                            .zip(destruct_variable(*vector, typ.clone()))
                                            .map(
                                                |(
                                                    ((element_part, _), (destination_part, _)),
                                                    (vector_part, _),
                                                )| {
                                                    Statement::Copy {
                                                        expression: Expression::Identifier(
                                                            destination_part,
                                                        ),
                                                        value: Shared::new(Value::VectorMutate {
                                                            vector: Shared::new(Value::Identifier(
                                                                vector_part,
                                                            )),
                                                            element: Shared::new(
                                                                Value::Identifier(element_part),
                                                            ),
                                                            index: index.clone(),
                                                        }),
                                                    }
                                                },
                                            )
                                            .map(Shared::new)
                                            .collect();
                                    } else {
                                        // element must be primitive, proceed with assignment
                                        return vec![Shared::new(Statement::Copy {
                                            expression: Expression::Identifier(
                                                destination.to_ident(),
                                            ),
                                            value: value.clone(),
                                        })];
                                    }
                                }
                                Value::CtorUnwrap {
                                    value: source,
                                    identifier: variant,
                                    ..
                                } => {
                                    // foo = bar as FooType;
                                    // =>
                                    // foo = bar_FooType;
                                    // =>
                                    // foo_a = bar_FooType_a;
                                    // foo_b = bar_FooType_b;

                                    let Value::Identifier(source) = &*source.get() else {
                                        // bail out
                                        // todo: warn here
                                        return vec![Shared::new(Statement::Copy {
                                            expression: Expression::Identifier(
                                                destination.to_ident(),
                                            ),
                                            value: value.clone(),
                                        })];
                                    };

                                    let Some(typ) =
                                        destructed_local_variables.get(&destination.root())
                                    else {
                                        // bail out
                                        // todo: warn here
                                        return vec![Shared::new(Statement::Copy {
                                            expression: Expression::Identifier(
                                                destination.to_ident(),
                                            ),
                                            value: value.clone(),
                                        })];
                                    };

                                    return destruct_variable(
                                        union_value_ident(*source, *variant),
                                        typ.clone(),
                                    )
                                    .into_iter()
                                    .zip(destruct_variable(destination.to_ident(), typ.clone()))
                                    .map(|((src, _), (dst, _))| Statement::Copy {
                                        expression: Expression::Identifier(dst),
                                        value: Shared::new(Value::Identifier(src)),
                                    })
                                    .map(Shared::new)
                                    .collect();
                                }
                                Value::Struct { name, .. } => {
                                    if let Some(dst_root_typ) = destructed_local_variables
                                        .get(&destination.root())
                                        .or_else(|| destructed_registers.get(&destination.root()))
                                    {
                                        let (dst_outer_ident, dst_outer_type) =
                                            traverse_typ_from_location(
                                                dst_root_typ.clone(),
                                                &destination,
                                            );

                                        let source_typ = composites.get(name).unwrap();

                                        return destruct_variable(dst_outer_ident, dst_outer_type)
                                            .into_iter()
                                            .zip(destruct_struct_value(
                                                value,
                                                source_typ.clone(),
                                                composites,
                                            ))
                                            .map(|((dst, _), src)| {
                                                Shared::new(Statement::Copy {
                                                    expression: Expression::Identifier(dst),
                                                    value: src.clone(),
                                                })
                                            })
                                            .collect();
                                    } else {
                                        return vec![Shared::new(Statement::Copy {
                                            expression: Expression::Identifier(
                                                destination.to_ident(),
                                            ),
                                            value: value.clone(),
                                        })];
                                    }
                                }

                                _ => {
                                    return vec![Shared::new(Statement::Copy {
                                        expression: Expression::Identifier(destination.to_ident()),
                                        value: value.clone(),
                                    })];
                                }
                            },
                        };

                        let source_type = destructed_local_variables
                            .get(&source.root())
                            .or_else(|| destructed_registers.get(&source.root()));
                        let dest_type = destructed_local_variables
                            .get(&destination.root())
                            .or_else(|| destructed_registers.get(&destination.root()));

                        match (source_type, dest_type) {
                            (Some(src_root_typ), Some(dst_root_typ)) => {
                                let (src_outer_ident, src_outer_type) =
                                    traverse_typ_from_location(src_root_typ.clone(), &source);
                                let (dst_outer_ident, dst_outer_type) =
                                    traverse_typ_from_location(dst_root_typ.clone(), &destination);

                                destruct_variable(src_outer_ident, src_outer_type)
                                    .into_iter()
                                    .zip(
                                        destruct_variable(dst_outer_ident, dst_outer_type)
                                            .into_iter(),
                                    )
                                    .map(|((src, _), (dst, _))| {
                                        let mut value = Shared::new(Value::Identifier(src));

                                        if let Some(index) = &vector_access {
                                            value = Shared::new(Value::VectorAccess {
                                                value,
                                                index: index.clone(),
                                            })
                                        }

                                        Statement::Copy {
                                            expression: Expression::Identifier(dst),
                                            value,
                                        }
                                    })
                                    .map(Shared::new)
                                    .collect()
                            }

                            (Some(src_root_typ), None) => {
                                let (src_outer_ident, _) =
                                    traverse_typ_from_location(src_root_typ.clone(), &source);

                                let DataLocation::Identifier(dst) = destination else {
                                    panic!();
                                };

                                let mut value = Shared::new(Value::Identifier(src_outer_ident));

                                if let Some(index) = &vector_access {
                                    value = Shared::new(Value::VectorAccess {
                                        value,
                                        index: index.clone(),
                                    })
                                }

                                vec![Shared::new(Statement::Copy {
                                    expression: Expression::Identifier(dst),
                                    value,
                                })]
                            }
                            (None, Some(dst_root_typ)) => {
                                let (dst_outer_ident, _) =
                                    traverse_typ_from_location(dst_root_typ.clone(), &destination);

                                let DataLocation::Identifier(src) = source else {
                                    panic!("non ident source: {source:?}");
                                };

                                let mut value = Shared::new(Value::Identifier(src));

                                if let Some(index) = &vector_access {
                                    value = Shared::new(Value::VectorAccess {
                                        value,
                                        index: index.clone(),
                                    })
                                }

                                vec![Shared::new(Statement::Copy {
                                    expression: Expression::Identifier(dst_outer_ident),
                                    value,
                                })]
                            }
                            (None, None) => vec![clone],
                        }
                    }

                    Statement::FunctionCall {
                        expression,
                        name,
                        arguments,
                    } => {
                        // function name is a union tag constructor
                        if let Some((union_typ, tag)) = is_union_constructor(*name, composites) {
                            create_union_construction_copies(expression, arguments, union_typ, tag)
                        } else {
                            let expression = expression
                                .clone()
                                // if we have an expression...
                                .map(|expr| {
                                    // turn it into a data location
                                    let destination = DataLocation::from(&expr);

                                    // if the function we're calling has a destructed return type
                                    if let Some(_) = destructed_return_types_by_function.get(name) {
                                        let root_typ = destructed_local_variables
                                        .get(&destination.root())
                                        .or_else(|| destructed_registers.get(&destination.root()))
                                        .unwrap_or_else(|| {
                                            panic!(
                                                "failed to find type for root of {destination:?}"
                                            )
                                        });
                                        let (dst_outer_ident, outer_typ) =
                                            traverse_typ_from_location(
                                                root_typ.clone(),
                                                &destination,
                                            );

                                        if is_type_composite(outer_typ.clone()) {
                                            // return a tuple assignment
                                            Expression::Tuple(
                                                destruct_variable(dst_outer_ident, outer_typ)
                                                    .into_iter()
                                                    .map(|(name, _)| Expression::Identifier(name))
                                                    .collect(),
                                            )
                                        } else {
                                            Expression::Identifier(dst_outer_ident)
                                        }
                                    } else {
                                        // otherwise treat as a single destination
                                        Expression::Identifier(destination.to_ident())
                                    }
                                });

                            let arguments = arguments
                                .iter()
                                .flat_map(|arg| {
                                    match &*arg.get() {
                                        Value::Identifier(ident) => {
                                            if let Some(typ) = destructed_local_variables
                                                .get(ident)
                                                .or_else(|| {
                                                    // unlikely but may as well check
                                                    destructed_registers.get(ident)
                                                })
                                            {
                                                destruct_variable(*ident, typ.clone())
                                                    .into_iter()
                                                    .map(|(new_ident, _)| {
                                                        Value::Identifier(new_ident)
                                                    })
                                                    .map(Shared::new)
                                                    .collect()
                                            } else {
                                                vec![arg.clone()]
                                            }
                                        }
                                        Value::Field { value, field_name } => {
                                            if let Value::Struct { fields, .. } = &*value.get() {
                                                vec![
                                                    fields
                                                        .iter()
                                                        .find(|nv| nv.name == *field_name)
                                                        .unwrap()
                                                        .value
                                                        .clone(),
                                                ]
                                            } else {
                                                todo!(":(");
                                            }
                                        }
                                        Value::Struct { fields, .. } => {
                                            fields
                                                .iter()
                                                .flat_map(|nv| {
                                                    let Value::Identifier(ident) = &*nv.value.get()
                                                    else {
                                                        todo!()
                                                    };

                                                    if let Some(typ) = destructed_local_variables
                                                        .get(ident)
                                                        .or_else(|| {
                                                            // unlikely but may as well check
                                                            destructed_registers.get(ident)
                                                        })
                                                    {
                                                        destruct_variable(*ident, typ.clone())
                                                            .into_iter()
                                                            .map(|(new_ident, _)| {
                                                                Value::Identifier(new_ident)
                                                            })
                                                            .map(Shared::new)
                                                            .collect()
                                                    } else {
                                                        vec![nv.value.clone()]
                                                    }
                                                })
                                                .collect()
                                        }
                                        _ => vec![arg.clone()],
                                    }
                                })
                                .collect();

                            vec![Shared::new(Statement::FunctionCall {
                                expression,
                                name: *name,
                                arguments,
                            })]
                        }
                    }

                    _ => vec![clone],
                }
            })
            .collect();

        block.set_statements(destructed);

        if let Terminator::Return(Some(Value::Identifier(return_value_ident))) = block.terminator()
        {
            // done to distinguish structs of one element (converted to unary tuple) from a
            // single return value
            if let Some(typ) = destructed_local_variables.get(&return_value_ident) {
                block.set_terminator(Terminator::Return(Some(Value::Tuple(
                    destruct_variable(return_value_ident, typ.clone())
                        .into_iter()
                        .map(|(name, _)| Value::Identifier(name))
                        .map(Shared::new)
                        .collect(),
                ))));
            }
        }
    });

    DestructorVisitor::new(destructed_local_variables).visit_function_definition(def);

    FieldSimplifier.visit_function_definition(def);
}

fn create_union_construction_copies(
    expression: &Option<Expression>,
    arguments: &Vec<Shared<Value>>,
    union_typ: Shared<Type>,
    tag: usize,
) -> Vec<Shared<Statement>> {
    let Some(Expression::Identifier(dst)) = expression else {
        panic!()
    };

    assert_eq!(1, arguments.len());

    let Type::Union { fields, .. } = &*union_typ.get() else {
        panic!();
    };

    let iter = [Shared::new(Statement::Copy {
        expression: Expression::Identifier(union_tag_ident(*dst)),
        value: Shared::new(Value::Literal(Shared::new(Literal::Int(tag.into())))),
    })]
    .into_iter();

    match &*arguments[0].get() {
        // assign args[0] to the correct value variable, splitting if necessary
        Value::Identifier(source) => {
            // need to write either the argument or a default_value to each field of the
            // union
            iter.chain(
                fields
                    .iter()
                    .enumerate()
                    .map(|(i, nt)| {
                        let src = if i == tag { Some(*source) } else { None };
                        let dst = union_value_ident(*dst, nt.name);
                        let typ = nt.typ.clone();

                        (src, dst, typ)
                    })
                    .flat_map(|(src, dst, typ)| {
                        if let Some(src) = src {
                            destruct_variable(src, typ.clone())
                                .into_iter()
                                .zip(destruct_variable(dst, typ).into_iter())
                                .map(|((src, _), (dst, _))| {
                                    (Shared::new(Value::Identifier(src)), dst)
                                })
                                .collect::<Vec<_>>()
                        } else {
                            destruct_variable(dst, typ)
                                .into_iter()
                                .map(|(dst, typ)| (default_value(typ), dst))
                                .collect()
                        }
                    })
                    .map(|(value, dst)| Statement::Copy {
                        expression: Expression::Identifier(dst),
                        value,
                    })
                    .map(Shared::new),
            )
            .collect()
        }
        Value::Literal(_) => iter
            .chain(
                fields
                    .iter()
                    .enumerate()
                    .map(|(i, nt)| {
                        let src = if i == tag {
                            Some(arguments[0].clone())
                        } else {
                            None
                        };
                        let dst = union_value_ident(*dst, nt.name);
                        let typ = nt.typ.clone();

                        (src, dst, typ)
                    })
                    .flat_map(|(src, dst, typ)| {
                        if let Some(src) = src {
                            // todo: handle struct literals?
                            vec![(src, dst)]
                        } else {
                            destruct_variable(dst, typ)
                                .into_iter()
                                .map(|(dst, typ)| (default_value(typ), dst))
                                .collect()
                        }
                    })
                    .map(|(value, dst)| Statement::Copy {
                        expression: Expression::Identifier(dst),
                        value,
                    })
                    .map(Shared::new),
            )
            .collect(),
        Value::Struct {
            fields: struct_fields,
            ..
        } => iter
            .chain(
                fields
                    .iter()
                    .enumerate()
                    .map(|(i, nt)| {
                        let src = if i == tag {
                            Some(arguments[0].clone())
                        } else {
                            None
                        };
                        let dst = union_value_ident(*dst, nt.name);
                        let typ = nt.typ.clone();

                        (src, dst, typ)
                    })
                    .flat_map(|(src, dst, typ)| {
                        if let Some(_) = src {
                            destruct_variable(dst, typ)
                                .into_iter()
                                .map(|(dst, _)| dst)
                                .zip(struct_fields.iter())
                                .map(|(dst, nv)| (nv.value.clone(), dst)) // todo: split nv.value if it's a composite
                                .collect::<Vec<_>>()
                        } else {
                            destruct_variable(dst, typ)
                                .into_iter()
                                .map(|(dst, typ)| (default_value(typ), dst))
                                .collect()
                        }
                    })
                    .map(|(value, dst)| Statement::Copy {
                        expression: Expression::Identifier(dst),
                        value,
                    })
                    .map(Shared::new),
            )
            .collect(),
        v => todo!("{v:?}"),
    }
}

/// Destructure a composite variable (name and type combination) into primitive
/// types with unique identifiers
fn destruct_variable(
    root_name: InternedString,
    typ: Shared<Type>,
) -> Vec<(InternedString, Shared<Type>)> {
    match &*typ.get() {
        Type::Struct { fields, .. } => fields
            .iter()
            .map(
                |NamedType {
                     name: field_name,
                     typ,
                 }| (struct_field_ident(root_name, *field_name), typ.clone()),
            )
            .collect::<Vec<_>>(),
        Type::Union { fields, .. } => {
            // create a tag variable
            [(
                union_tag_ident(root_name),
                Shared::new(Type::Integer {
                    size: Size::Static(64),
                }),
            )]
            .into_iter()
            .chain(
                // and value variables for each variant
                fields
                    .iter()
                    .enumerate()
                    .map(|(_tag, NamedType { name, typ })| {
                        (union_value_ident(root_name, *name), typ.clone())
                    }),
            )
            .collect()
        }
        Type::FixedVector {
            length,
            element_type,
        } => {
            if is_type_composite(element_type.clone()) {
                destruct_variable(root_name, element_type.clone())
                    .into_iter()
                    .map(|(name, typ)| {
                        (
                            name,
                            Shared::new(Type::FixedVector {
                                length: *length,
                                element_type: typ,
                            }),
                        )
                    })
                    .collect()
            } else {
                vec![(root_name, typ.clone())]
            }
        }
        Type::Vector { element_type } => {
            if is_type_composite(element_type.clone()) {
                destruct_variable(root_name, element_type.clone())
                    .into_iter()
                    .map(|(name, typ)| (name, Shared::new(Type::Vector { element_type: typ })))
                    .collect()
            } else {
                vec![(root_name, typ.clone())]
            }
        }
        _ => vec![(root_name, typ.clone())],
    }
    .into_iter()
    .map(|(name, typ)| {
        if is_type_composite(typ.clone()) {
            destruct_variable(name, typ.clone())
        } else {
            vec![(name, typ)]
        }
    })
    .flatten()
    .collect()
}

/// Identifier for a union's tag
pub fn union_tag_ident(ident: InternedString) -> InternedString {
    format!("{ident}_tag").into()
}

/// Identifier for a union's values
pub fn union_value_ident(root: InternedString, variant: InternedString) -> InternedString {
    format!("{root}_{variant}").into()
}

/// Identifier for a struct's values
pub fn struct_field_ident(root: InternedString, field: InternedString) -> InternedString {
    format!("{root}_{field}").into()
}

fn default_value(typ: Shared<Type>) -> Shared<Value> {
    match &*typ.get() {
        Type::Unit => Shared::new(Value::Literal(Shared::new(Literal::Unit))),
        Type::String => Shared::new(Value::Literal(Shared::new(Literal::String(
            InternedString::from_static("default value"),
        )))),
        Type::Bool => Shared::new(Value::Literal(Shared::new(Literal::Bool(false)))),
        Type::Integer { .. } => Shared::new(Value::Literal(Shared::new(Literal::Int(0.into())))),
        Type::Bits { size } => Shared::new(Value::Literal(Shared::new(Literal::Bits(vec![
            0.into(
            );
            match size {
                Size::Static(n) => *n,
                Size::Unknown => 1,
            }
        ])))),
        Type::Struct { name, .. } | Type::Union { name, .. } => {
            panic!("no default value for composite type {name:?}, it should be destructed")
        }
        t => todo!("{t:?}"),
    }
}

/// If the supplied identifier is a constructor for a union variant, return the
/// fields of that union and the tag of that variant
fn is_union_constructor(
    ident: InternedString,
    composites: &HashMap<InternedString, Shared<Type>>,
) -> Option<(Shared<Type>, usize)> {
    composites
        .values()
        .filter_map(|c| {
            if let Type::Union { .. } = &*c.get() {
                Some(c.clone())
            } else {
                None
            }
        })
        .flat_map(|typ| {
            let cloned = typ.clone();
            let Type::Union { fields, .. } = &*cloned.get() else {
                unreachable!()
            };
            fields
                .clone()
                .into_iter()
                .enumerate()
                .map(move |(i, nt)| (typ.clone(), i, nt))
        })
        .find(|(_, _, nt)| nt.name == ident)
        .map(|(typ, tag, _)| (typ, tag))
}

fn is_type_composite(typ: Shared<Type>) -> bool {
    match &*typ.get() {
        Type::Union { .. } | Type::Struct { .. } => true,
        Type::FixedVector { element_type, .. } | Type::Vector { element_type } => {
            is_type_composite(element_type.clone())
        }
        _ => false,
    }
}

fn destruct_function_parameters(
    ast: Shared<Ast>,
) -> HashMap<InternedString, HashMap<InternedString, Shared<Type>>> {
    ast.get_mut()
        .functions
        .iter_mut()
        // // filter out functions with no composite parameters
        // .filter(|(_, def)| {
        //     def.signature
        //         .parameters
        //         .get()
        //         .iter()
        //         .any(|p| is_type_composite(p.typ.clone()).is_some())
        // })
        .map(|(name, def)| {
            let mut destructed_parameters = HashMap::default();

            let new_parameters = def
                .signature
                .parameters
                .get()
                .clone()
                .into_iter()
                .map(|p| (p.name, p.typ.clone(), is_type_composite(p.typ)))
                .map(|(name, typ, is_composite)| {
                    if is_composite {
                        destructed_parameters.insert(name, typ.clone());
                        destruct_variable(name, typ)
                    } else {
                        vec![(name, typ)]
                    }
                })
                .flatten()
                .map(|(name, typ)| Parameter { name, typ })
                .collect::<Vec<_>>();

            *def.signature.parameters.get_mut() = new_parameters;

            //  = Some(Shared::new(Type::Tuple(
            // destruct_variable("".into(), &kind)
            //     .into_iter()
            //     .map(|(_, typ)| typ)
            //     .collect(),

            (*name, destructed_parameters)
        })
        .collect::<HashMap<_, _>>()
}

fn destruct_function_return_types(ast: Shared<Ast>) -> HashMap<InternedString, Shared<Type>> {
    ast.get_mut()
        .functions
        .iter_mut()
        .filter(|(_, def)| {
            matches!(
                def.signature
                    .return_type
                    .as_ref()
                    .map(|t| is_type_composite(t.clone())),
                Some(true)
            )
        })
        .map(|(name, def)| {
            let typ = def.signature.return_type.clone().unwrap();

            def.signature.return_type = Some(Shared::new(Type::Tuple(
                destruct_variable("return".into(), typ.clone())
                    .into_iter()
                    .map(|(_, typ)| typ)
                    .collect(),
            )));

            (*name, typ)
        })
        .collect::<HashMap<_, _>>()
}

fn destruct_registers(ast: Shared<Ast>) -> HashMap<InternedString, Shared<Type>> {
    let composite_registers = ast
        .get_mut()
        .registers
        .iter()
        .filter(|(_, typ)| is_type_composite((*typ).clone()))
        .map(|(name, typ)| (*name, typ.clone()))
        .collect::<HashMap<_, _>>();

    for (register_name, typ) in &composite_registers {
        ast.get_mut().registers.remove(register_name);

        ast.get_mut()
            .registers
            .extend(destruct_variable(*register_name, typ.clone()));
    }

    composite_registers
}

/// Determine the identifer and type given a root typ and a data location
///
/// todo: explain that better
fn traverse_typ_from_location(
    typ: Shared<Type>,
    location: &DataLocation,
) -> (InternedString, Shared<Type>) {
    match location {
        // data location is of type Kind
        DataLocation::Identifier(name) => (*name, typ),
        //
        DataLocation::Fields { root, fields } => {
            let mut current_type = typ.get().clone();
            let mut current_identifier = root.clone();

            for field in fields {
                match current_type.clone() {
                    Type::Union { fields: _, .. } => {
                        // let next = fields.iter().find(|nt| nt.name == *field).unwrap();
                        // current_type = next.typ.get().clone();
                        // current_identifier = union_value_ident(current_identifier, *field);
                        todo!();
                    }

                    Type::Struct {
                        fields,
                        name: struct_name,
                    } => {
                        let next = fields.iter().find(|nt| nt.name == *field).unwrap_or_else(|| panic!("failed to find {field:?} as a field name of {struct_name:?} in {typ:?}"));
                        current_type = next.typ.get().clone();
                        current_identifier = struct_field_ident(current_identifier, *field);
                    }

                    _ => panic!(),
                }
            }

            (current_identifier, Shared::new(current_type))
        }
    }
}

// fn destruct_expression_destination(
//     expression: &Expression,
//     destructed_registers: &HashMap<InternedString, Kind>,
//     destructed_variables: &HashMap<InternedString, Kind>,
// ) -> Expression {
//     match expression {
//         Expression::Identifier(ident) => {
//             if let Some(fields) = destructed_registers
//                 .get(ident)
//                 .or_else(|| destructed_variables.get(ident))
//             {
//                 Expression::Tuple(
//                     destruct_variable(*ident, fields)
//                         .into_iter()
//                         .map(|(ident, _)| Expression::Identifier(ident))
//                         .collect(),
//                 )
//             } else {
//                 Expression::Identifier(*ident)
//             }
//         }
//         Expression::Field { expression, field } => match &**expression {
//             Expression::Identifier(root) => {
//                 Expression::Identifier(struct_field_ident(*root, *field))
//             }
//             Expression::Field { expression, field } => todo!("gotta go
// deeper"),             _ => panic!(),
//         },
//         _ => panic!(),
//     }
// }

struct DestructorVisitor {
    destructed: HashMap<InternedString, Shared<Type>>,
}

impl DestructorVisitor {
    fn new(destructed: HashMap<InternedString, Shared<Type>>) -> Self {
        Self { destructed }
    }
}

impl Visitor for DestructorVisitor {
    fn visit_value(&mut self, node: Shared<Value>) {
        let node = &mut *node.get_mut();

        if let Value::Operation(op) = node.clone() {
            match op {
                Operation::Equal(left, right) => {
                    if let (Value::Identifier(left), Value::Identifier(right)) =
                        (&*left.get(), &*right.get())
                    {
                        if let (Some(left_type), Some(right_type)) =
                            (self.destructed.get(&*left), self.destructed.get(&*right))
                        {
                            let left_components = destruct_variable(*left, left_type.clone());
                            let right_components = destruct_variable(*right, right_type.clone());

                            let equals = left_components
                                .into_iter()
                                .filter(|(_, typ)| !matches!(*typ.get(), Type::Unit))
                                .map(|(name, _)| Value::Identifier(name))
                                .map(Shared::new)
                                .zip(
                                    right_components
                                        .into_iter()
                                        .filter(|(_, typ)| !matches!(*typ.get(), Type::Unit))
                                        .map(|(name, _)| Value::Identifier(name))
                                        .map(Shared::new),
                                )
                                .map(|(left, right)| {
                                    Value::Operation(Operation::Equal(left, right))
                                })
                                .collect::<Vec<_>>();

                            *node = match &equals[..] {
                                [] => unreachable!(),
                                [op] => op.clone(),
                                [first, second, ..] => {
                                    let init = Value::Operation(Operation::And(
                                        Shared::new(first.clone()),
                                        Shared::new(second.clone()),
                                    ));

                                    equals.into_iter().skip(2).map(Shared::new).fold(
                                        init,
                                        |acc, next| {
                                            Value::Operation(Operation::And(Shared::new(acc), next))
                                        },
                                    )
                                }
                            };

                            return;
                        }
                    }
                }
                // Operation::NotEqual   (left, right) => {},//todo
                _ => (),
            }
        }

        node.walk(self);
    }
}

fn destruct_struct_value(
    value: &Shared<Value>,
    typ: Shared<Type>,
    composites: &HashMap<InternedString, Shared<Type>>,
) -> Vec<Shared<Value>> {
    match &*value.get() {
        Value::Identifier(ident) => destruct_variable(*ident, typ)
            .into_iter()
            .map(|(ident, _)| Shared::new(Value::Identifier(ident)))
            .collect(),
        Value::Struct {
            fields: value_fields,
            name,
        } => {
            let Type::Struct {
                fields: type_fields,
                ..
            } = &*composites.get(name).unwrap().get()
            else {
                unreachable!()
            };

            value_fields
                .iter()
                .zip(type_fields.iter())
                .flat_map(|(nv, nt)| destruct_struct_value(&nv.value, nt.typ.clone(), composites))
                .collect()
        }
        _ => vec![value.clone()],
    }
}

struct FieldSimplifier;

impl Visitor for FieldSimplifier {
    fn visit_value(&mut self, node: Shared<Value>) {
        let node = &mut *node.get_mut();

        if let Value::Field { value, field_name } = node.clone() {
            if let Value::Struct { fields, .. } = &*value.get() {
                let new_value = fields
                    .iter()
                    .find(|nv| nv.name == field_name)
                    .unwrap()
                    .value
                    .get()
                    .clone();

                *node = new_value;
            }
        }

        node.walk(self);
    }
}
