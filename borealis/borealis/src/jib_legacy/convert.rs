//! JIB to BOOM conversion

use {
    crate::{
        boom::{
            self, Bit, Expression, FunctionDefinition, FunctionSignature, NamedType, Parameter,
            Size, Statement, Type, Value,
            control_flow::{ControlFlowBlock, Terminator},
        },
        jib_legacy::{control_flow::ControlFlowGraphBuilder, convert::sail_ast::Identifier},
    },
    common::{hashmap::HashMap, intern::InternedString},
    itertools::Itertools,
    sailrs::{
        jib_ast::{self, CReturn},
        sail_ast,
        shared::Shared,
    },
    std::borrow::Borrow,
};

type Parameters = Vec<Shared<boom::Type>>;
type Return = Shared<boom::Type>;

/// Converts JIB AST into BOOM AST
pub fn jib_to_boom<I: IntoIterator<Item = jib_ast::Definition>>(iter: I) -> Shared<boom::Ast> {
    let mut emitter = BoomEmitter::new();
    emitter.process(iter);

    let mut ast = emitter.finish();

    {
        ast.registers
            .insert("have_exception".into(), Shared::new(Type::Bool));
        ast.registers.insert(
            "current_exception".into(),
            Shared::new(Type::Union {
                name: InternedString::from_static("exception"),
                fields: ast
                    .unions
                    .get(&InternedString::from_static("exception"))
                    .unwrap()
                    .clone(),
            }),
        );
        ast.registers
            .insert("throw_location".into(), Shared::new(Type::String));
    }

    {
        let return_type = Shared::new(Type::Struct {
            name: "tuple#%bv_%bv4".into(),
            fields: ast
                .structs
                .get(&InternedString::from("tuple#%bv_%bv4"))
                .unwrap()
                .clone(),
        });
        let entry_block = ControlFlowBlock::new();
        entry_block.set_statements(vec![
            Shared::new(Statement::VariableDeclaration {
                name: "return".into(),
                typ: return_type.clone(),
            }),
            Shared::new(Statement::FunctionCall {
                expression: Some(Expression::Identifier("return".into())),
                name: "AddWithCarry".into(),
                arguments: vec![
                    Shared::new(Value::Identifier("x".into())),
                    Shared::new(Value::Identifier("y".into())),
                    Shared::new(Value::Identifier("carry_in".into())),
                ],
            }),
        ]);
        entry_block.set_terminator(Terminator::Return(Some(Value::Identifier("return".into()))));
        ast.functions.insert(
            "add_with_carry_test".into(),
            FunctionDefinition {
                signature: FunctionSignature {
                    name: "add_with_carry_test".into(),
                    parameters: Shared::new(vec![
                        Parameter {
                            name: "x".into(),
                            typ: Shared::new(Type::Bits {
                                size: Size::Static(64),
                            }),
                        },
                        Parameter {
                            name: "y".into(),
                            typ: Shared::new(Type::Bits {
                                size: Size::Static(64),
                            }),
                        },
                        Parameter {
                            name: "carry_in".into(),
                            typ: Shared::new(Type::Bits {
                                size: Size::Static(1),
                            }),
                        },
                    ]),
                    return_type: Some(return_type),
                },
                entry_block,
            },
        );
    }

    Shared::new(ast)
}

/// Consumes JIB AST and produces BOOM
#[derive(Debug, Default)]
pub struct BoomEmitter {
    /// BOOM AST being constructed by walker
    ast: boom::Ast,
    /// Temporarily stored type signatures as spec and function definitions are
    /// separate
    function_types: HashMap<InternedString, (Parameters, Return)>,
    /// Register initialization statements (also letbinds)
    register_init_statements: Vec<Shared<boom::Statement>>,
}

impl BoomEmitter {
    /// Create a new `BoomEmitter`
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a sequence of JIB definitions
    /// IntoParallelIterator
    pub fn process<I: IntoIterator<Item = jib_ast::Definition>>(&mut self, definitions: I) {
        definitions
            .into_iter() //.into_par_iter
            .for_each(|def| self.process_definition(&def));
    }

    /// Emit BOOM AST
    pub fn finish(mut self) -> boom::Ast {
        // create register initialization function
        {
            self.register_init_statements
                .push(Shared::new(boom::Statement::VariableDeclaration {
                    name: "ret".into(),
                    typ: Shared::new(Type::Unit),
                }));
            self.register_init_statements
                .push(Shared::new(boom::Statement::Copy {
                    expression: boom::Expression::Identifier("ret".into()),
                    value: Shared::new(boom::Value::Literal(Shared::new(boom::Literal::Unit))),
                }));
            self.register_init_statements
                .push(Shared::new(boom::Statement::End("ret".into())));

            self.ast.functions.insert(
                "borealis_register_init".into(),
                FunctionDefinition {
                    signature: FunctionSignature {
                        name: "borealis_register_init".into(),
                        parameters: Shared::new(vec![]),
                        return_type: None,
                    },
                    entry_block: ControlFlowGraphBuilder::from_statements(
                        &self.register_init_statements,
                    ),
                },
            );
        }

        self.ast.functions.extend(self.function_types.iter().map(
            |(name, (parameters, return_type))| {
                (
                    *name,
                    FunctionDefinition {
                        signature: FunctionSignature {
                            name: *name,
                            parameters: Shared::new(
                                parameters
                                    .iter()
                                    .enumerate()
                                    .map(|(i, typ)| Parameter {
                                        name: format!("p{i}").into(),
                                        typ: typ.clone(),
                                    })
                                    .collect(),
                            ),
                            return_type: Some(return_type.clone()),
                        },
                        entry_block: ControlFlowBlock::new(),
                    },
                )
            },
        ));

        self.ast
    }

    fn process_definition(&mut self, definition: &jib_ast::Definition) {
        match &definition.def {
            jib_ast::DefinitionAux::Register(ident, typ, body) => {
                self.ast
                    .registers
                    .insert(ident.as_interned(), convert_type(typ));
                self.register_init_statements
                    .extend_from_slice(&convert_body(body.as_ref()));
            }
            jib_ast::DefinitionAux::Type(type_def) => {
                match type_def {
                    jib_ast::TypeDefinition::Enum(name, variants) => {
                        self.ast.enums.insert(
                            name.as_interned(),
                            variants.iter().map(Identifier::as_interned).collect(),
                        );
                    } /* type is u32 but don't need to */
                    // define it
                    jib_ast::TypeDefinition::Struct(name, fields) => {
                        self.ast
                            .structs
                            .insert(name.as_interned(), convert_fields(fields.iter()));
                    }
                    jib_ast::TypeDefinition::Variant(name, fields) => {
                        self.ast
                            .unions
                            .insert(name.as_interned(), convert_fields(fields.iter()));
                    }
                }
            }
            jib_ast::DefinitionAux::Let(_, bindings, body) => {
                bindings.iter().for_each(|(ident, typ)| {
                    self.ast
                        .registers
                        .insert(ident.as_interned(), convert_type(typ));
                });
                self.register_init_statements
                    .extend_from_slice(&convert_body(body.as_ref()));
            }
            jib_ast::DefinitionAux::Val(id, _, parameters, out) => {
                self.function_types.insert(
                    id.as_interned(),
                    (
                        parameters.iter().map(convert_type).collect(),
                        convert_type(out),
                    ),
                );
            }
            jib_ast::DefinitionAux::Fundef(name, _, arguments, body) => {
                let (parameter_types, return_type) =
                    self.function_types.remove(&name.as_interned()).unwrap();

                let parameters = Shared::new(
                    arguments
                        .iter()
                        .map(sail_ast::Identifier::as_interned)
                        .zip(parameter_types)
                        .map(|(name, typ)| Parameter { name, typ })
                        .collect::<Vec<_>>(),
                );

                let name = name.as_interned();

                let mut body = convert_body(body.as_ref());

                // make implicit return variable explicit
                body.insert(
                    0,
                    Shared::new(boom::Statement::VariableDeclaration {
                        name: "return".into(),
                        typ: return_type.clone(),
                    }),
                );

                //debug!("building new control flow graph for {name}");
                let control_flow = ControlFlowGraphBuilder::from_statements(&body);

                self.ast.functions.insert(
                    name,
                    boom::FunctionDefinition {
                        signature: FunctionSignature {
                            name,
                            parameters,
                            return_type: Some(return_type),
                        },
                        entry_block: control_flow,
                    },
                );
            }
            jib_ast::DefinitionAux::Startup(_, _) => todo!(),
            jib_ast::DefinitionAux::Finish(_, _) => todo!(),
            jib_ast::DefinitionAux::Pragma(key, value) => {
                self.ast.pragmas.insert(*key, *value);
            }
        };
    }
}

fn convert_type<T: Borrow<jib_ast::Type>>(typ: T) -> Shared<boom::Type> {
    Shared::new(match typ.borrow() {
        jib_ast::Type::Lbits => boom::Type::Bits {
            size: Size::Unknown,
        },
        jib_ast::Type::Fbits(i) => boom::Type::Bits {
            size: Size::Static(usize::try_from(*i).unwrap()),
        },

        jib_ast::Type::Lint => boom::Type::Integer {
            size: Size::Unknown,
        },
        jib_ast::Type::Fint(i) => boom::Type::Integer {
            size: Size::Static(usize::try_from(*i).unwrap()),
        },

        jib_ast::Type::Unit => boom::Type::Unit,
        jib_ast::Type::Bool => boom::Type::Bool,
        jib_ast::Type::Bit => boom::Type::Bit,
        jib_ast::Type::String => boom::Type::String,
        jib_ast::Type::Real => boom::Type::Real,

        // enums are basically constants
        jib_ast::Type::Enum(_, _) => boom::Type::Integer {
            size: Size::Static(32),
        },

        // need to destruct these
        jib_ast::Type::Struct(name, fields) => boom::Type::Struct {
            name: name.as_interned(),
            fields: convert_fields(fields.as_ref()),
        },

        // unions are special
        jib_ast::Type::Variant(name, fields) => boom::Type::Union {
            name: name.as_interned(),
            fields: convert_fields(fields.as_ref()),
        },
        jib_ast::Type::Fvector(length, typ) => boom::Type::FixedVector {
            length: usize::try_from(*length).unwrap(),
            element_type: convert_type(&**typ),
        },
        jib_ast::Type::Vector(typ) => boom::Type::Vector {
            element_type: (convert_type(&**typ)),
        },
        jib_ast::Type::List(typ) => boom::Type::Vector {
            element_type: (convert_type(&**typ)),
        },
        jib_ast::Type::Ref(typ) => boom::Type::Reference(convert_type(&**typ)),
        jib_ast::Type::Constant(c) => boom::Type::Constant((&c.0).try_into().unwrap()),
        t => todo!("jib type {t:?}"),
    })
}

fn convert_body(instructions: &[jib_ast::Instruction]) -> Vec<Shared<boom::Statement>> {
    instructions
        .iter()
        .flat_map(|instr| convert_statement(&instr.inner))
        .collect()
}

fn convert_statement(statement: &jib_ast::InstructionAux) -> Vec<Shared<boom::Statement>> {
    if let jib_ast::InstructionAux::Block(instructions)
    | jib_ast::InstructionAux::TryBlock(instructions) = statement
    {
        return convert_body(instructions.as_ref());
    }

    let statements = match statement {
        jib_ast::InstructionAux::Decl(typ, name) => vec![boom::Statement::VariableDeclaration {
            name: convert_name(name),
            typ: convert_type(typ),
        }],
        jib_ast::InstructionAux::Init(typ, name, value) => {
            vec![
                boom::Statement::VariableDeclaration {
                    name: convert_name(name),
                    typ: convert_type(typ),
                },
                boom::Statement::Copy {
                    expression: boom::Expression::Identifier(convert_name(name)),
                    value: convert_value(value),
                },
            ]
        }
        jib_ast::InstructionAux::Jump(condition, target) => vec![boom::Statement::Jump {
            condition: convert_value(condition),
            target: *target,
        }],
        jib_ast::InstructionAux::Goto(s) => vec![boom::Statement::Goto(*s)],
        jib_ast::InstructionAux::Label(s) => vec![boom::Statement::Label(*s)],
        jib_ast::InstructionAux::Funcall(ret, _, (name, _), args) => {
            let CReturn::One(expression) = ret else {
                todo!()
            };
            vec![boom::Statement::FunctionCall {
                expression: convert_expression(expression),
                name: name.as_interned(),
                arguments: args.iter().map(convert_value).collect(),
            }]
        }
        jib_ast::InstructionAux::Copy(expression, value) => vec![boom::Statement::Copy {
            expression: convert_expression(expression).unwrap(),
            value: convert_value(value),
        }],
        jib_ast::InstructionAux::Clear(_, _) => vec![],
        jib_ast::InstructionAux::Undefined(_) => vec![boom::Statement::Undefined],
        jib_ast::InstructionAux::Exit(s) => vec![boom::Statement::Exit(*s)],
        jib_ast::InstructionAux::End(name) => vec![boom::Statement::End(convert_name(name))],
        jib_ast::InstructionAux::If(condition, if_body, else_body, _) => {
            vec![boom::Statement::If {
                condition: convert_value(condition),
                if_body: convert_body(if_body.as_ref()),
                else_body: convert_body(else_body.as_ref()),
            }]
        }

        jib_ast::InstructionAux::Throw(value) => {
            vec![boom::Statement::Panic(convert_value(value))]
        }
        jib_ast::InstructionAux::Comment(s) => vec![boom::Statement::Comment(*s)],
        jib_ast::InstructionAux::TryBlock(_) | jib_ast::InstructionAux::Block(_) => unreachable!(),
        jib_ast::InstructionAux::Raw(_) => todo!(),
        jib_ast::InstructionAux::Return(_) => todo!(),
        jib_ast::InstructionAux::Reset(_, _) => todo!(),
        jib_ast::InstructionAux::Reinit(_, _, _) => todo!(),
    };

    statements.into_iter().map(Shared::new).collect()
}

fn convert_name(name: &jib_ast::Name) -> InternedString {
    match name {
        jib_ast::Name::Name(ident, _) => ident.as_interned(),
        jib_ast::Name::HaveException(_) => InternedString::from_static("have_exception"),
        jib_ast::Name::CurrentException(_) => InternedString::from_static("current_exception"),
        jib_ast::Name::ThrowLocation(_) => InternedString::from_static("throw_location"),
        jib_ast::Name::Return(_) => InternedString::from_static("return"),
        jib_ast::Name::Channel(_, _) => InternedString::from_static("channel"),
    }
}

fn convert_expression(expression: &jib_ast::Expression) -> Option<boom::Expression> {
    match expression {
        jib_ast::Expression::Id(name, _) => Some(boom::Expression::Identifier(convert_name(name))),
        jib_ast::Expression::Rmw(_, _, _) => todo!(),
        jib_ast::Expression::Field(expression, ident) => Some(boom::Expression::Field {
            expression: Box::new(convert_expression(expression).unwrap()),
            field: ident.as_interned(),
        }),
        jib_ast::Expression::Addr(expr) => Some(boom::Expression::Address(Box::new(
            convert_expression(expr).unwrap(),
        ))),
        jib_ast::Expression::Tuple(_, _) => todo!(),
        jib_ast::Expression::Void => None,
    }
}

fn convert_value(value: &jib_ast::Value) -> Shared<boom::Value> {
    Shared::new(match value {
        jib_ast::Value::Id(name, _) => boom::Value::Identifier(convert_name(name)),
        jib_ast::Value::Lit(vl, _) => boom::Value::Literal(convert_literal(vl)),
        jib_ast::Value::Tuple(_, _) => todo!(),
        jib_ast::Value::Struct(fields, jib_ast::Type::Struct(ident, _)) => boom::Value::Struct {
            name: ident.as_interned(),
            fields: fields
                .iter()
                .map(|(ident, value)| boom::NamedValue {
                    name: ident.as_interned(),
                    value: convert_value(value),
                })
                .collect(),
        },
        jib_ast::Value::Struct(_, _) => panic!("encountered struct with non-struct type"),
        jib_ast::Value::CtorKind(value, (ctor, unifiers), _) => boom::Value::CtorKind {
            value: (convert_value(value)),
            identifier: ctor.as_interned(),
        },
        jib_ast::Value::CtorUnwrap(value, (ctor, unifiers), _) => boom::Value::CtorUnwrap {
            value: (convert_value(value)),
            identifier: ctor.as_interned(),
        },
        jib_ast::Value::TupleMember(_, _, _) => todo!(),
        jib_ast::Value::Call(op, values) => {
            let values = values.iter().map(convert_value).collect::<Vec<_>>();

            let op = match op {
                jib_ast::Op::Bnot => boom::Operation::Not(values[0].clone()),
                jib_ast::Op::Bor => todo!(),
                jib_ast::Op::Band => todo!(),
                jib_ast::Op::ListHead => todo!(),
                jib_ast::Op::ListTail => todo!(),
                jib_ast::Op::Eq => todo!(),
                jib_ast::Op::Neq => boom::Operation::Not(Shared::new(boom::Value::Operation(
                    boom::Operation::Equal(values[0].clone(), values[1].clone()),
                ))),
                jib_ast::Op::Ite => todo!(),
                jib_ast::Op::Ilt => boom::Operation::LessThan(values[0].clone(), values[1].clone()),

                jib_ast::Op::Ilteq => todo!(),
                jib_ast::Op::Igt => {
                    boom::Operation::GreaterThan(values[0].clone(), values[1].clone())
                }
                jib_ast::Op::Igteq => todo!(),
                jib_ast::Op::Iadd => boom::Operation::Add(values[0].clone(), values[1].clone()),
                jib_ast::Op::Isub => {
                    boom::Operation::Subtract(values[0].clone(), values[1].clone())
                }
                jib_ast::Op::Unsigned(_) => todo!(),
                jib_ast::Op::Signed(_) => todo!(),
                jib_ast::Op::Bvnot => todo!(),
                jib_ast::Op::Bvor => todo!(),
                jib_ast::Op::Bvand => todo!(),
                jib_ast::Op::Bvxor => todo!(),
                jib_ast::Op::Bvadd => todo!(),
                jib_ast::Op::Bvsub => todo!(),
                jib_ast::Op::Bvaccess => todo!(),
                jib_ast::Op::Concat => todo!(),
                jib_ast::Op::ZeroExtend(_) => todo!(),
                jib_ast::Op::SignExtend(_) => todo!(),
                jib_ast::Op::Slice(_) => todo!(),
                jib_ast::Op::Sslice(_) => todo!(),
                jib_ast::Op::SetSlice => todo!(),
                jib_ast::Op::Replicate(_) => todo!(),
                jib_ast::Op::ListIsEmpty => todo!(),
            };
            boom::Value::Operation(op)
        }
        jib_ast::Value::Field(value, ident) => boom::Value::Field {
            value: (convert_value(value)),
            field_name: ident.as_interned(),
        },
        // convert enum members into their indices as integer literals
        jib_ast::Value::Member(ident, typ) => {
            let jib_ast::Type::Enum(_, members) = typ else {
                todo!()
            };

            let member_index = members
                .iter()
                .find_position(|s| s.as_interned() == ident.as_interned())
                .unwrap_or_else(|| {
                    panic!("failed to find index for enum {ident:?} of type {typ:?}")
                })
                .0;

            boom::Value::Literal(Shared::new(boom::Literal::Int(member_index.into())))
        }
    })
}

fn convert_literal(literal: &jib_ast::Vl) -> Shared<boom::Literal> {
    Shared::new(match literal {
        jib_ast::Vl::Bits(bits) => {
            // this may need a `.rev`
            // update 2024-03-21: turns out it does on sail17arm94
            boom::Literal::Bits(bits.iter().rev().map(convert_bit).collect())
        }
        jib_ast::Vl::Bit(bit) => boom::Literal::Bit(convert_bit(bit)),
        jib_ast::Vl::Bool(b) => boom::Literal::Bool(*b),
        jib_ast::Vl::Unit => boom::Literal::Unit,
        jib_ast::Vl::Int(bigint) => boom::Literal::Int(bigint.0.clone()),
        jib_ast::Vl::String(s) => boom::Literal::String(*s),
        jib_ast::Vl::Real(_) => todo!(),
        jib_ast::Vl::Enum(_) => todo!(),
        jib_ast::Vl::Ref(s) => boom::Literal::Reference(*s),
        jib_ast::Vl::Undefined => boom::Literal::Undefined,
    })
}

fn convert_bit(bit: &jib_ast::BitU) -> boom::Bit {
    match bit {
        jib_ast::BitU::B0 => Bit::Zero,
        jib_ast::BitU::B1 => Bit::One,
        jib_ast::BitU::BU => Bit::Unknown,
    }
}

/// Converts fields of a struct or union from JIB to BOOM
///
/// Generics are required to be able to convert from `LinkedList<((Identifier,
/// LinkedList<Type>), Box<Type>)>` *and* `LinkedList<((Identifier,
/// LinkedList<Type>), Type)>`.
fn convert_fields<
    'a,
    TYPE: Borrow<jib_ast::Type> + 'a,
    ITER: IntoIterator<Item = &'a (sail_ast::Identifier, TYPE)>,
>(
    fields: ITER,
) -> Vec<NamedType> {
    fields
        .into_iter()
        .map(|(name, typ)| NamedType {
            name: name.as_interned(),
            typ: convert_type(typ.borrow()),
        })
        .collect()
}
