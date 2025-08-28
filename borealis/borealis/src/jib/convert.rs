//! JIB to BOOM conversion

use {
    crate::boom::{
        self, Expression, FunctionDefinition, FunctionSignature, NamedType, Parameter, Size,
        Statement, Type, Value,
        control_flow::{ControlFlowBlock, Terminator},
    },
    common::{hashmap::HashMap, intern::InternedString},
    isla_lib::{
        bitvector::{BV, b64::B64},
        ir::{Def, Exp, Instr, Loc, Ty},
    },
    num_bigint::BigInt,
    sailrs::shared::Shared,
    std::{borrow::Borrow, collections::BTreeMap},
};

type Parameters = Vec<Shared<boom::Type>>;
type Return = Shared<boom::Type>;

/// Converts JIB AST into BOOM AST
pub fn jib_to_boom<I: IntoIterator<Item = Def<InternedString, B64>>>(iter: I) -> Shared<boom::Ast> {
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
                .unwrap_or_else(|| panic!("{:?}", ast.structs.keys().collect::<Vec<_>>()))
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
    pub fn process<I: IntoIterator<Item = Def<InternedString, B64>>>(&mut self, definitions: I) {
        definitions
            .into_iter() //.into_par_iter
            .for_each(|def| self.process_definition(&def));
    }

    /// Emit BOOM AST
    pub fn finish(mut self) -> boom::Ast {
        // create register initialization function
        {
            let entry_block = ControlFlowBlock::new();

            entry_block.set_statements(self.register_init_statements);
            entry_block.set_terminator(boom::control_flow::Terminator::Return(None));

            self.ast.functions.insert(
                "borealis_register_init".into(),
                FunctionDefinition {
                    signature: FunctionSignature {
                        name: "borealis_register_init".into(),
                        parameters: Shared::new(vec![]),
                        return_type: None,
                    },
                    entry_block,
                },
            );
        }

        // external functions
        // todo: handle this better from IR
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

    fn process_definition(&mut self, definition: &Def<InternedString, B64>) {
        match definition {
            Def::Register(ident, typ, body) => {
                self.ast.registers.insert(*ident, self.convert_type(typ));
                let mut statements = body
                    .iter()
                    .flat_map(|i| self.convert_instruction(i))
                    .collect::<Vec<_>>();
                self.register_init_statements.append(&mut statements);
            }
            Def::Enum(name, variants) => {
                self.ast.enums.insert(*name, variants.clone());
            }
            Def::Struct(name, fields) => {
                self.ast
                    .structs
                    .insert(*name, self.convert_fields(fields.iter()));
            }
            Def::Union(name, fields) => {
                self.ast
                    .unions
                    .insert(*name, self.convert_fields(fields.iter()));
            }
            Def::Let(bindings, body) => {
                bindings.iter().for_each(|(ident, typ)| {
                    self.ast.registers.insert(*ident, self.convert_type(typ));
                });
                let mut statements = body
                    .iter()
                    .flat_map(|i| self.convert_instruction(i))
                    .collect::<Vec<_>>();
                self.register_init_statements.append(&mut statements);
            }
            Def::Extern(id, _, _, parameters, out) | Def::Val(id, parameters, out) => {
                self.function_types.insert(
                    *id,
                    (
                        parameters.iter().map(|t| self.convert_type(t)).collect(),
                        self.convert_type(out),
                    ),
                );
            }
            Def::Fn(name, arguments, body) => {
                let (parameter_types, return_type) = self.function_types.remove(name).unwrap();

                let parameters = Shared::new(
                    arguments
                        .iter()
                        .copied()
                        .zip(parameter_types)
                        .map(|(name, typ)| Parameter { name, typ })
                        .collect::<Vec<_>>(),
                );

                let entry_block = self.convert_body(body.as_ref());

                // // make implicit return variable explicit
                let mut statements = entry_block.statements();
                statements.insert(
                    0,
                    Shared::new(boom::Statement::VariableDeclaration {
                        name: "return".into(),
                        typ: return_type.clone(),
                    }),
                );
                entry_block.set_statements(statements);

                self.ast.functions.insert(
                    *name,
                    boom::FunctionDefinition {
                        signature: FunctionSignature {
                            name: *name,
                            parameters,
                            return_type: Some(return_type),
                        },
                        entry_block,
                    },
                );
            }
            Def::Pragma(key, value) => {
                self.ast.pragmas.insert(
                    InternedString::from(key.as_str()),
                    InternedString::from(value.as_str()),
                );
            }

            Def::Files(items) => log::trace!("files: {items:?}"),
        };
    }

    /// Converts fields of a struct or union from JIB to BOOM
    ///
    /// Generics are required to be able to convert from
    /// `LinkedList<((Identifier, LinkedList<Type>), Box<Type>)>` *and*
    /// `LinkedList<((Identifier, LinkedList<Type>), Type)>`.
    fn convert_fields<
        'a,
        TYPE: Borrow<Ty<InternedString>> + 'a,
        ITER: IntoIterator<Item = &'a (InternedString, TYPE)>,
    >(
        &self,
        fields: ITER,
    ) -> Vec<NamedType> {
        fields
            .into_iter()
            .map(|(name, typ)| NamedType {
                name: *name,
                typ: self.convert_type(typ.borrow()),
            })
            .collect()
    }

    fn convert_type<T: Borrow<Ty<InternedString>>>(&self, typ: T) -> Shared<boom::Type> {
        Shared::new(match typ.borrow() {
            Ty::I64 => boom::Type::Integer {
                size: Size::Static(64),
            },
            Ty::I128 => boom::Type::Integer {
                size: Size::Static(64), // :(
            },
            Ty::AnyBits => boom::Type::Bits {
                size: Size::Unknown,
            },
            Ty::Bits(width) => boom::Type::Bits {
                size: Size::Static(usize::try_from(*width).unwrap()),
            },
            Ty::Float(_fpty) => boom::Type::Float, // todo: properly handle floating point type

            Ty::Unit => boom::Type::Unit,
            Ty::Bool => boom::Type::Bool,
            Ty::Bit => boom::Type::Bit,
            Ty::String => boom::Type::String,
            Ty::Real => boom::Type::Real,
            Ty::RoundingMode => boom::Type::RoundingMode,

            Ty::FixedVector(length, ty) => boom::Type::FixedVector {
                length: usize::try_from(*length).unwrap(),
                element_type: self.convert_type(&**ty),
            },
            Ty::Vector(ty) | Ty::List(ty) => boom::Type::Vector {
                element_type: (self.convert_type(&**ty)),
            },
            Ty::Ref(ty) => self.convert_type(&**ty).get().clone(),

            // enums are constants
            Ty::Enum(_) => boom::Type::Integer {
                size: Size::Static(32),
            },
            Ty::Struct(name) => boom::Type::Struct {
                name: *name,
                fields: self.ast.structs.get(name).unwrap().clone(),
            },
            Ty::Union(name) => boom::Type::Union {
                name: *name,
                fields: self.ast.unions.get(name).unwrap().clone(),
            },
        })
    }
    fn convert_body(&self, instructions: &[Instr<InternedString, B64>]) -> ControlFlowBlock {
        // pre-scan for jumps and gotos to allow for out-of-order jumping
        let block_locations = instructions
            .iter()
            .enumerate()
            .flat_map(|(index, instruction)| match instruction {
                Instr::Goto(target) | Instr::Jump(_, target, _) => [
                    Some((*target, ControlFlowBlock::new())),
                    Some((index + 1, ControlFlowBlock::new())),
                ],
                Instr::End | Instr::Exit(_, _) => {
                    [Some((index + 1, ControlFlowBlock::new())), None]
                }
                _ => [None, None],
            })
            .filter_map(|a| a)
            .collect::<BTreeMap<_, _>>();

        let entry = ControlFlowBlock::new();

        let mut current_block = entry.clone();
        let mut current_statements = vec![];

        // for every instruction in the body
        // todo: rewrite this as a for each
        let mut iter = instructions.iter().enumerate();
        while let Some((idx, instr)) = iter.next() {
            // if the current index was the target of a jump, start a new block
            if let Some(next_block) = block_locations.get(&idx) {
                // only start a new block if we're not already on the correct block
                if next_block.id() != current_block.id() {
                    current_block.set_statements(current_statements.clone());
                    current_statements.clear();

                    current_block.set_terminator(boom::control_flow::Terminator::Unconditional {
                        target: next_block.clone(),
                    });
                    next_block.add_parent(&current_block);

                    current_block = block_locations.get(&idx).unwrap().clone();
                }
            }

            match instr {
                // unconditional jump
                Instr::Goto(target) => {
                    let target_block = block_locations.get(target).unwrap().clone();

                    current_block.set_statements(current_statements.clone());
                    current_statements.clear();

                    current_block.set_terminator(boom::control_flow::Terminator::Unconditional {
                        target: target_block.clone(),
                    });
                    target_block.add_parent(&current_block);

                    current_block = block_locations.get(&(idx + 1)).unwrap().clone();
                }

                // conditional jump
                Instr::Jump(condition, target, _) => {
                    let fallthrough_block = block_locations.get(&(idx + 1)).unwrap().clone();

                    let target_block = block_locations.get(target).unwrap().clone();

                    current_block.set_statements(current_statements.clone());
                    current_statements.clear();

                    current_block.set_terminator(boom::control_flow::Terminator::Conditional {
                        condition: self.convert_expression(condition).get().clone(),
                        target: target_block.clone(),
                        fallthrough: fallthrough_block.clone(),
                    });
                    target_block.add_parent(&current_block);

                    current_block = fallthrough_block;
                }
                // return
                Instr::End => {
                    current_block.set_statements(current_statements.clone());
                    current_statements.clear();

                    current_block.set_terminator(boom::control_flow::Terminator::Return(Some(
                        boom::Value::Identifier("return".into()),
                    )));

                    current_block = block_locations.get(&(idx + 1)).unwrap().clone();
                }
                // panic
                Instr::Exit(cause, _) => {
                    current_block.set_statements(current_statements.clone());
                    current_statements.clear();

                    current_block.set_terminator(boom::control_flow::Terminator::Panic(
                        boom::Value::Literal(Shared::new(boom::Literal::String(
                            format!("{cause:?}").into(),
                        ))),
                    ));

                    current_block = block_locations.get(&(idx + 1)).unwrap().clone();
                }
                _ => current_statements.extend_from_slice(&self.convert_instruction(instr)),
            }
        }

        entry
    }

    fn convert_instruction(
        &self,
        instr: &Instr<InternedString, B64>,
    ) -> Vec<Shared<boom::Statement>> {
        let statements = match instr {
            Instr::Decl(name, ty, _) => vec![boom::Statement::VariableDeclaration {
                name: *name,
                typ: self.convert_type(ty),
            }],
            Instr::Init(name, ty, exp, _) => {
                vec![
                    boom::Statement::VariableDeclaration {
                        name: *name,
                        typ: self.convert_type(ty),
                    },
                    boom::Statement::Copy {
                        expression: boom::Expression::Identifier(*name),
                        value: self.convert_expression(exp),
                    },
                ]
            }

            Instr::Copy(loc, exp, _) => {
                vec![boom::Statement::Copy {
                    expression: convert_location(loc),
                    value: self.convert_expression(exp),
                }]
            }

            Instr::Call(loc, _, name, args, _) => {
                let expression = convert_location(loc);
                vec![boom::Statement::FunctionCall {
                    expression: Some(expression),
                    name: *name,
                    arguments: args.iter().map(|a| self.convert_expression(a)).collect(),
                }]
            }

            Instr::Monomorphize(..) => todo!(),
            Instr::PrimopUnary(..) => todo!(),
            Instr::PrimopBinary(..) => todo!(),
            Instr::PrimopVariadic(..) => todo!(),
            Instr::PrimopReset(..) => todo!(),

            Instr::Arbitrary => vec![boom::Statement::Undefined],

            Instr::Jump(..) => unreachable!("jump"),
            Instr::Goto(_) => unreachable!("goto"),
            Instr::Exit(..) => unreachable!("exit"),
            Instr::End => unreachable!("end"),
        };

        statements.into_iter().map(Shared::new).collect()
    }

    fn convert_expression(&self, expression: &Exp<InternedString>) -> Shared<boom::Value> {
        Shared::new(match expression {
            Exp::Id(id) => {
                if let Some((variant_index, _)) = self
                    .ast
                    .enums
                    .values()
                    .flat_map(|variants| variants.iter().enumerate())
                    .find(|(_, variant)| *variant == id)
                {
                    boom::Value::Literal(Shared::new(boom::Literal::Int(variant_index.into())))
                } else {
                    boom::Value::Identifier(*id)
                }
            }
            Exp::Ref(id) => boom::Value::Identifier(*id), /* todo: fix this */
            Exp::Bool(b) => boom::Value::Literal(Shared::new(boom::Literal::Bool(*b))),
            Exp::Bits(bits) => boom::Value::Literal(Shared::new(boom::Literal::Bits(
                // todo: use bits type in rest of codebase
                bits.to_vec()
                    .into_iter()
                    .map(|b| if b { boom::Bit::One } else { boom::Bit::Zero })
                    .collect(),
            ))),
            Exp::String(s) => {
                boom::Value::Literal(Shared::new(boom::Literal::String(s.as_str().into())))
            }
            Exp::Unit => boom::Value::Literal(Shared::new(boom::Literal::Unit)),
            Exp::I64(i) => boom::Value::Literal(Shared::new(boom::Literal::Int(BigInt::from(*i)))),
            Exp::I128(i) => boom::Value::Literal(Shared::new(boom::Literal::Int(BigInt::from(*i)))),
            Exp::Undefined(_ty) => boom::Value::Literal(Shared::new(boom::Literal::Undefined)), /* todo: use type somehow? */
            Exp::Struct(name, fields) => boom::Value::Struct {
                name: *name,
                fields: fields
                    .iter()
                    .map(|(ident, value)| boom::NamedValue {
                        name: *ident,
                        value: self.convert_expression(value),
                    })
                    .collect(),
            },
            Exp::Kind(name, exp) => boom::Value::CtorKind {
                value: self.convert_expression(exp),
                identifier: *name,
            },
            Exp::Unwrap(name, exp) => boom::Value::CtorUnwrap {
                value: self.convert_expression(exp),
                identifier: *name,
            },
            Exp::Field(exp, field) => boom::Value::Field {
                value: self.convert_expression(exp),
                field_name: *field,
            },
            Exp::Call(op, values) => {
                let values = values
                    .iter()
                    .map(|v| self.convert_expression(v))
                    .collect::<Vec<_>>();

                let op = match op {
                    isla_lib::ir::Op::Not => boom::Operation::Not(values[0].clone()),
                    isla_lib::ir::Op::Or => todo!(),
                    isla_lib::ir::Op::And => todo!(),
                    isla_lib::ir::Op::Eq => todo!(),
                    isla_lib::ir::Op::Neq => {
                        boom::Operation::Not(Shared::new(boom::Value::Operation(
                            boom::Operation::Equal(values[0].clone(), values[1].clone()),
                        )))
                    }
                    isla_lib::ir::Op::Lteq => todo!(),
                    isla_lib::ir::Op::Lt => {
                        boom::Operation::LessThan(values[0].clone(), values[1].clone())
                    }
                    isla_lib::ir::Op::Gteq => todo!(),
                    isla_lib::ir::Op::Gt => {
                        boom::Operation::GreaterThan(values[0].clone(), values[1].clone())
                    }
                    isla_lib::ir::Op::Add => {
                        boom::Operation::Add(values[0].clone(), values[1].clone())
                    }
                    isla_lib::ir::Op::Sub => {
                        boom::Operation::Subtract(values[0].clone(), values[1].clone())
                    }
                    isla_lib::ir::Op::Slice(_) => todo!(),
                    isla_lib::ir::Op::SetSlice => todo!(),
                    isla_lib::ir::Op::Signed(_) => todo!(),
                    isla_lib::ir::Op::Unsigned(_) => todo!(),
                    isla_lib::ir::Op::ZeroExtend(_) => todo!(),
                    isla_lib::ir::Op::Bvnot => todo!(),
                    isla_lib::ir::Op::Bvor => todo!(),
                    isla_lib::ir::Op::Bvxor => todo!(),
                    isla_lib::ir::Op::Bvand => todo!(),
                    isla_lib::ir::Op::Bvadd => todo!(),
                    isla_lib::ir::Op::Bvsub => todo!(),
                    isla_lib::ir::Op::Bvaccess => todo!(),
                    isla_lib::ir::Op::Concat => todo!(),
                    isla_lib::ir::Op::Head => todo!(),
                    isla_lib::ir::Op::Tail => todo!(),
                    isla_lib::ir::Op::IsEmpty => todo!(),
                };

                boom::Value::Operation(op)
            }
        })
    }
}

fn convert_location(location: &Loc<InternedString>) -> boom::Expression {
    match location {
        Loc::Id(id) => boom::Expression::Identifier(*id),
        Loc::Field(loc, field) => boom::Expression::Field {
            expression: Box::new(convert_location(loc)),
            field: *field,
        },
        Loc::Addr(loc) => boom::Expression::Address(Box::new(convert_location(loc))),
    }
}
