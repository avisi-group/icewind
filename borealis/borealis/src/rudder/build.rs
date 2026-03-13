use {
    crate::boom::{self, Expression, bits_to_int, passes::destruct_composites},
    common::{
        arena::{Arena, Ref},
        hashmap::HashMap,
        id::Id,
        intern::InternedString,
        rudder::{
            Model, RegisterCacheType, RegisterDescriptor,
            block::Block,
            constant::Constant,
            function::{Function, Symbol},
            statement::{
                BinaryOperationKind, CastOperationKind, ShiftOperationKind, Statement,
                TernaryOperationKind, UnaryOperationKind, build, cast,
            },
            types::{PrimitiveType, Type},
        },
        width_helpers::signed_smallest_width_of_value,
    },
    core::panic,
    log::trace,
    rayon::iter::{IntoParallelIterator, ParallelIterator},
    sailrs::shared::Shared,
    std::cmp::Ordering,
};

pub fn from_boom(ast: &boom::Ast) -> Model {
    let mut build_ctx = BuildContext::default();

    ast.registers.iter().for_each(|(name, typ)| {
        let typ = build_ctx.resolve_type(typ.clone());
        build_ctx.add_register(*name, typ);
    });

    // need all functions with signatures before building
    ast.functions
        .iter()
        .for_each(|(name, definition)| build_ctx.add_function(*name, definition));

    build_ctx.unions = ast
        .unions
        .values()
        .flat_map(|variants| {
            variants.iter().enumerate().map(|(i, nt)| {
                (
                    nt.name,
                    (
                        if let boom::Type::Unit = &*nt.typ.get() {
                            None
                        } else {
                            Some(build_ctx.resolve_type(nt.typ.clone()))
                        },
                        u32::try_from(i).unwrap(),
                    ),
                )
            })
        })
        .collect::<HashMap<_, _>>();

    // insert replicate bits signature

    log::warn!("starting build functions");
    let model = build_ctx.build_functions();
    log::warn!("done build functions");

    model
}

#[derive(Default)]
struct BuildContext {
    /// Union variant to type and tag map
    unions: HashMap<InternedString, (Option<Type>, u32)>,

    /// Register name to type and offset mapping
    registers: HashMap<InternedString, RegisterDescriptor>,
    next_register_offset: u64,

    /// Functions
    functions: HashMap<InternedString, (Function, boom::FunctionDefinition)>,
}

impl BuildContext {
    fn add_register(&mut self, name: InternedString, typ: Type) {
        self.registers.insert(
            name,
            RegisterDescriptor {
                typ: typ.clone(),
                offset: self.next_register_offset,
                cache: RegisterCacheType::None,
            },
        );

        log::debug!(
            "adding register {name} ({} bytes) @ {:#x}",
            typ.width_bytes(),
            self.next_register_offset
        );

        // 8 byte aligned
        self.next_register_offset += u64::from(typ.width_bytes()).next_multiple_of(8)
    }

    fn add_function(&mut self, name: InternedString, definition: &boom::FunctionDefinition) {
        self.functions.insert(
            name,
            (
                Function::new(
                    name,
                    definition
                        .signature
                        .return_type
                        .as_ref()
                        .map(|typ| self.resolve_type(typ.clone())),
                    definition
                        .signature
                        .parameters
                        .get()
                        .iter()
                        .map(|boom::Parameter { typ, name }| {
                            Symbol::new(*name, self.resolve_type(typ.clone()))
                        })
                        .collect(),
                ),
                definition.clone(),
            ),
        );
    }

    fn build_functions(self) -> Model {
        let fns = self
            .functions
            .clone()
            .into_par_iter()
            .map(|(name, (rudder_fn, boom_fn))| {
                log::debug!("building function {name:?}");
                (
                    name,
                    FunctionBuildContext::new(&self, rudder_fn).build_fn(boom_fn.clone()),
                )
            })
            .collect();

        Model::new(
            fns,
            // register names kept for debugging
            self.registers,
        )
    }

    fn resolve_type(&self, typ: Shared<boom::Type>) -> Type {
        match &*typ.get() {
            boom::Type::Unit => panic!("found unit"),
            boom::Type::RoundingMode => todo!(),
            boom::Type::String => Type::String,
            // value
            boom::Type::Bool | boom::Type::Bit => Type::u1(),
            boom::Type::Float => Type::f64(),
            boom::Type::Union { .. } | boom::Type::Struct { .. } => {
                // todo: panic
                log::warn!("should be removed by pass: {:?}", &*typ.get());
                Type::new_primitive(PrimitiveType::UnsignedInteger(9999))
            }

            boom::Type::Real => Type::Real,

            boom::Type::Vector { element_type } => {
                let element_type = (self.resolve_type(element_type.clone())).clone();
                // todo: Brian Campbell said the Sail C backend had functionality to staticize
                // all bitvector lengths
                element_type.vectorize(0)
            }
            boom::Type::FixedVector {
                length,
                element_type,
            } => {
                let element_type = (self.resolve_type(element_type.clone())).clone();

                element_type.vectorize(usize::try_from(*length).unwrap())
            }
            boom::Type::Reference(inner) => {
                // todo: this is broken:(
                self.resolve_type(inner.clone())
            }
            boom::Type::Integer { size } => match size {
                boom::Size::Static(size) => {
                    Type::new_primitive(PrimitiveType::SignedInteger(u32::try_from(*size).unwrap()))
                }
                boom::Size::Unknown => Type::Int,
            },
            boom::Type::Bits { size } => match size {
                boom::Size::Static(size) => Type::new_primitive(PrimitiveType::UnsignedInteger(
                    u32::try_from(*size).unwrap(),
                )),
                boom::Size::Unknown => Type::Bits,
            },
            boom::Type::Constant(c) => {
                // todo: this should be a panic, but because structs/unions can have constant
                // type fields we do the following
                Type::new_primitive(PrimitiveType::SignedInteger(
                    signed_smallest_width_of_value(*c),
                ))
            }
            boom::Type::Tuple(ts) => {
                Type::Tuple(ts.iter().cloned().map(|t| self.resolve_type(t)).collect())
            }
        }
    }
}

struct FunctionBuildContext<'ctx> {
    build_context: &'ctx BuildContext,
    rudder_fn: Function,
    blocks: HashMap<Id, Ref<Block>>,
}

impl<'ctx> FunctionBuildContext<'ctx> {
    pub fn new(build_context: &'ctx BuildContext, rudder_fn: Function) -> Self {
        Self {
            build_context,
            rudder_fn,
            blocks: HashMap::default(),
        }
    }

    pub fn build_fn(mut self, boom_fn: boom::FunctionDefinition) -> Function {
        trace!(
            "converting function {:?} from boom to rudder",
            boom_fn.signature.name
        );
        let entry = self.resolve_block(boom_fn.entry_block);
        self.rudder_fn.set_entry_block(entry);

        self.rudder_fn
    }

    pub fn resolve_block(
        &mut self,
        boom_block: boom::control_flow::ControlFlowBlock,
    ) -> Ref<Block> {
        trace!("resolving: {:x}", boom_block.id());

        if let Some(block) = self.blocks.get(&boom_block.id()) {
            trace!("already resolved: {:x}", boom_block.id());
            block.clone()
        } else {
            trace!("building: {:x}", boom_block.id());
            BlockBuildContext::new(self).build_block(boom_block)
        }
    }
}

struct BlockBuildContext<'ctx, 'fn_ctx> {
    function_build_context: &'fn_ctx mut FunctionBuildContext<'ctx>,
    block: Ref<Block>,
}

impl<'ctx: 'fn_ctx, 'fn_ctx> BlockBuildContext<'ctx, 'fn_ctx> {
    pub fn new(function_build_context: &'fn_ctx mut FunctionBuildContext<'ctx>) -> Self {
        let block = function_build_context.rudder_fn.new_block();

        Self {
            function_build_context,
            block,
        }
    }

    fn ctx(&mut self) -> &BuildContext {
        self.function_build_context.build_context
    }

    fn fn_ctx(&self) -> &FunctionBuildContext<'ctx> {
        self.function_build_context
    }

    fn fn_ctx_mut(&mut self) -> &mut FunctionBuildContext<'ctx> {
        self.function_build_context
    }

    fn block_arena(&self) -> &Arena<Block> {
        self.fn_ctx().rudder_fn.arena()
    }

    fn block_arena_mut(&mut self) -> &mut Arena<Block> {
        self.fn_ctx_mut().rudder_fn.arena_mut()
    }

    fn statement_arena(&self) -> &Arena<Statement> {
        self.block.get(self.block_arena()).arena()
    }

    fn build_block(mut self, boom_block: boom::control_flow::ControlFlowBlock) -> Ref<Block> {
        // pre-insert empty rudder block to avoid infinite recursion with cyclic blocks
        {
            let rudder_block = self.block.clone();
            self.fn_ctx_mut()
                .blocks
                .insert(boom_block.id(), rudder_block);
        }

        // convert statements
        boom_block
            .statements()
            .iter()
            .for_each(|stmt| self.build_statement(stmt.clone()));

        // check terminator, insert final rudder statement
        let kind = match boom_block.terminator() {
            boom::control_flow::Terminator::Return(value) => Statement::Return {
                value: value.map(|value| self.build_value(Shared::new(value))),
            },

            boom::control_flow::Terminator::Conditional {
                condition,
                target: boom_target,
                fallthrough: boom_fallthrough,
            } => {
                let condition = self.build_value(Shared::new(condition));
                let condition = cast(self.block, self.block_arena_mut(), condition, Type::u1());

                let rudder_true_target = self.fn_ctx_mut().resolve_block(boom_target);
                let rudder_false_target = self.fn_ctx_mut().resolve_block(boom_fallthrough);

                Statement::Branch {
                    condition,
                    true_target: rudder_true_target,
                    false_target: rudder_false_target,
                }
            }
            boom::control_flow::Terminator::Unconditional {
                target: boom_target,
            } => {
                let rudder_target = self.fn_ctx_mut().resolve_block(boom_target);
                Statement::Jump {
                    target: rudder_target,
                }
            }
            boom::control_flow::Terminator::Panic(value) => {
                Statement::Panic(self.build_value(Shared::new(value.clone())))
            }
        };

        build(self.block, self.block_arena_mut(), kind);

        self.block
    }

    fn build_statement(&mut self, statement: Shared<boom::Statement>) {
        match &*statement.get() {
            boom::Statement::VariableDeclaration { name, typ } => {
                let typ = self.ctx().resolve_type(typ.clone());
                self.fn_ctx_mut()
                    .rudder_fn
                    .add_local_variable(Symbol::new(*name, typ));
            }
            boom::Statement::Copy { expression, value } => {
                self.build_copy(value.clone(), expression);
            }
            boom::Statement::FunctionCall {
                expression,
                name,
                arguments,
            } => {
                self.build_function_call(arguments, name, expression);
            }

            boom::Statement::Label(_)
            | boom::Statement::Goto(_)
            | boom::Statement::Jump { .. }
            | boom::Statement::End(_)
            | boom::Statement::Undefined
            | boom::Statement::If { .. } => {
                panic!("no control flow should exist at this point in compilation!\n{statement:?}")
            }
            boom::Statement::Comment(_) => (),
            boom::Statement::Exit(msg) => {
                let value = self.build_value(Shared::new(boom::Value::Literal(Shared::new(
                    boom::Literal::String(*msg),
                ))));
                build(self.block, self.block_arena_mut(), Statement::Panic(value));
            }
            boom::Statement::Panic(value) => {
                let value = self.build_value(value.clone());
                build(self.block, self.block_arena_mut(), Statement::Panic(value));
            }
        }
    }

    fn build_copy(&mut self, value: Shared<boom::Value>, expression: &boom::Expression) {
        let source = self.build_value(value.clone());

        self.build_expression_write(expression, source);
    }

    fn build_function_call(
        &mut self,
        arguments: &[Shared<boom::Value>],
        name: &InternedString,
        expression: &Option<boom::Expression>,
    ) {
        let args = arguments
            .iter()
            .map(|arg| self.build_value(arg.clone()))
            .collect::<Vec<_>>();

        let fn_statement = {
            if let Some(statement) = self.build_specialized_function(*name, &args, expression) {
                statement
            } else {
                let return_type = self
                    .ctx()
                    .functions
                    .get(name)
                    .unwrap_or_else(|| panic!("unknown function {name:?}"))
                    .0
                    .return_type();

                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Call {
                        target: *name,
                        args,
                        return_type,
                    },
                )
            }
        };

        if let Some(expression) = expression {
            match expression {
                boom::Expression::Tuple(exprs) => {
                    exprs.iter().enumerate().for_each(|(index, expression)| {
                        let tuple_field = build(
                            self.block,
                            self.block_arena_mut(),
                            Statement::TupleAccess {
                                index,
                                source: fn_statement.clone(),
                            },
                        );
                        self.build_expression_write(expression, tuple_field);
                    })
                }
                _ => self.build_expression_write(expression, fn_statement),
            }
        }
    }

    /// Sail compiler builtin functions only!
    fn build_specialized_function(
        &mut self,
        name: InternedString,
        args: &[Ref<Statement>],
        expression: &Option<boom::Expression>, /* occasionally needed to find destination type
                                                * of function */
    ) -> Option<Ref<Statement>> {
        match name.as_ref() {
            "%i64->%i" => {
                // lots of %i64->%i(Int(BigInt(-1))) so disabled this check
                // assert_eq!(Type::s64(), *args[0].typ());
                Some(args[0].clone())
            }

            "%i->%i64" => {
                let arena = self.statement_arena();
                assert_eq!(args[0].get(arena).typ(arena), Some(Type::s64()));

                Some(args[0].clone())
            }

            // "%string->%real" => {
            //     let Statement::Constant { value, .. } = args[0].get(self.statement_arena()) else
            // {         panic!();
            //     };

            //     let ConstantValue::String(str) = value else {
            //         panic!();
            //     };

            //     let r = Ratio::<i128>::from_f64(str.as_ref().parse().unwrap()).unwrap();

            //     Some(build(
            //         self.block,
            //         self.block_arena_mut(),
            //         Statement::Constant {
            //             typ: (Type::Rational),
            //             value: ConstantValue::Rational(r),
            //         },
            //     ))
            // }
            "make_the_value" | "size_itself_int" => Some(args[0].clone()),
            // %bv, %i, %i -> %bv
            "subrange_bits" => {
                // end - start + 1
                let one = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_signed(1, 64)),
                );

                let typ = {
                    let arena = self.statement_arena();
                    args[1].get(arena).typ(arena)
                };
                let one = cast(self.block, self.block_arena_mut(), one, typ.unwrap());
                let diff = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::Sub,
                        lhs: args[1].clone(),
                        rhs: args[2].clone(),
                    },
                );
                let len = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::Add,
                        lhs: diff.clone(),
                        rhs: one.clone(),
                    },
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitExtract {
                        value: args[0].clone(),
                        start: args[2].clone(),
                        width: len,
                    },
                ))
            }

            "undefined_range" => Some(args[0].clone()),

            "eq_bit" | "eq_bits" | "eq_int" | "eq_bool" | "eq_string" | "eq_real" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::CompareEqual,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),

            "neq_bits"
            | "neq_any<ESecurityState%>"
            | "neq_any<EFault%>"
            | "neq_any<EMemOp%>"
            | "neq_bool"
            | "neq_int" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::CompareNotEqual,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),

            // val add_atom : (%i, %i) -> %i
            // val add_bits : (%bv, %bv) -> %bv
            // val add_real : (%real, %real) -> %real
            "add_atom" | "add_bits" | "add_real" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::Add,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),

            // val add_bits_int : (%bv, %i) -> %bv
            "add_bits_int" => {
                let rhs = cast(
                    self.block,
                    self.block_arena_mut(),
                    args[1].clone(),
                    Type::u64(),
                );
                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::Add,
                        lhs: args[0].clone(),
                        rhs,
                    },
                ))
            }

            // val sub_bits_int : (%bv, %i) -> %bv
            "sub_bits_int" => {
                let rhs = cast(
                    self.block,
                    self.block_arena_mut(),
                    args[1].clone(),
                    Type::u64(),
                );
                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::Sub,
                        lhs: args[0].clone(),
                        rhs,
                    },
                ))
            }

            "sub_bits" | "sub_atom" | "sub_real" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::Sub,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),

            "mult_atom" | "mult_real" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::Multiply,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),

            "tdiv_int" | "ediv_int" | "ediv_nat" | "div_real" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::Divide,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),

            "emod_nat" | "_builtin_mod_nat" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::Modulo,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),

            "negate_atom" | "neg_real" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::UnaryOperation {
                    kind: UnaryOperationKind::Negate,
                    value: args[0].clone(),
                },
            )),
            "abs_int_atom" | "abs_real" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::UnaryOperation {
                    kind: UnaryOperationKind::Absolute,
                    value: args[0].clone(),
                },
            )),
            "min_int" => {
                let left = args[0].clone();
                let right = args[1].clone();

                let condition = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::CompareLessThan,
                        lhs: left.clone(),
                        rhs: right.clone(),
                    },
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Select {
                        condition,
                        true_value: left,
                        false_value: right,
                    },
                ))
            }

            "max_int" => {
                let true_value = args[0].clone();
                let false_value = args[1].clone();

                let condition = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::CompareGreaterThan,
                        lhs: true_value.clone(),
                        rhs: false_value.clone(),
                    },
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Select {
                        condition,
                        true_value,
                        false_value,
                    },
                ))
            }

            // val ceil : (%real) -> %i
            "ceil" => {
                let ceil = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::UnaryOperation {
                        kind: UnaryOperationKind::Ceil,
                        value: args[0].clone(),
                    },
                );
                Some(ceil)
            }

            // val floor : (%real) -> %i
            "floor" => {
                let floor = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::UnaryOperation {
                        kind: UnaryOperationKind::Floor,
                        value: args[0].clone(),
                    },
                );
                Some(floor)
            }

            // val pow2 : (%i) -> %i
            // val _builtin_pow2 : (%i) -> %i
            "pow2" | "_builtin_pow2" => {
                // WRONG!! pow2(n) is 2^n not n^2
                // Some(build(   self.block, self.block_arena_mut(),Statement::UnaryOperation {
                //     kind: UnaryOperationKind::Power2,
                //     value: args[0].clone(),
                // }))

                // hopefully correct
                // 1 << args[0]
                let const_1 = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_signed(1, 64)),
                );
                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::ShiftOperation {
                        kind: ShiftOperationKind::LogicalShiftLeft,
                        value: const_1,
                        amount: args[0].clone(),
                    },
                ))
            }

            // val pow_real : (%real, %i) -> %real
            "pow_real" => {
                // cast args[1] to i32, todo: move this to codegen cause it's a rust thing
                let i = cast(
                    self.block,
                    self.block_arena_mut(),
                    args[1].clone(),
                    Type::s32(),
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::PowI,
                        lhs: args[0].clone(),
                        rhs: i,
                    },
                ))
            }

            "sqrt" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::UnaryOperation {
                    kind: UnaryOperationKind::SquareRoot,
                    value: args[0].clone(),
                },
            )),

            "lt_int" | "lt_real" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::CompareLessThan,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),
            "lteq_int" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::CompareLessThanOrEqual,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),
            "gt_int" | "gt_real" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::CompareGreaterThan,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),
            "gteq_int" | "gteq_real" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::CompareGreaterThanOrEqual,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),
            "not_bool" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::UnaryOperation {
                    kind: UnaryOperationKind::Not,
                    value: args[0].clone(),
                },
            )),
            "not_vec" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::UnaryOperation {
                    kind: UnaryOperationKind::Complement,
                    value: args[0].clone(),
                },
            )),
            "and_vec" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::And,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),
            "xor_vec" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::Xor,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),
            "or_vec" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BinaryOperation {
                    kind: BinaryOperationKind::Or,
                    lhs: args[0].clone(),
                    rhs: args[1].clone(),
                },
            )),

            "sail_shiftright" | "_shr_int" | "_shr32" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::ShiftOperation {
                    kind: ShiftOperationKind::LogicalShiftRight,
                    value: args[0].clone(),
                    amount: args[1].clone(),
                },
            )),
            "sail_arith_shiftright" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::ShiftOperation {
                    kind: ShiftOperationKind::ArithmeticShiftRight,
                    value: args[0].clone(),
                    amount: args[1].clone(),
                },
            )),
            "sail_shiftleft" | "_shl_int" | "_shl8" | "_shl32" | "_shl1" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::ShiftOperation {
                    kind: ShiftOperationKind::LogicalShiftLeft,
                    value: args[0].clone(),
                    amount: args[1].clone(),
                },
            )),

            "slice" => {
                // uint64 n, uint64 start, uint64 len
                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitExtract {
                        value: args[0].clone(),
                        start: args[1].clone(),
                        width: args[2].clone(),
                    },
                ))
            }

            "bitvector_access" => {
                let length = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_unsigned(1, 64)),
                );
                let bitex = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitExtract {
                        value: args[0].clone(),
                        start: args[1].clone(),
                        width: length,
                    },
                );

                Some(cast(self.block, self.block_arena_mut(), bitex, Type::u1()))
            }

            // val undefined_bitvector : (%i) -> %bv
            "undefined_bitvector" => {
                let zero = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_unsigned(0, 64)),
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::CreateBits {
                        value: zero,
                        width: args[0].clone(),
                    },
                ))
            }

            "bitvector_length" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::SizeOf {
                    value: args[0].clone(),
                },
            )),

            "update_fbits" => {
                //     if ((bit & 1) == 1) {
                //         return op | (bit << n);
                //    } else {
                //         return op & ~(bit << n);
                //    }
                let op = args[0].clone();
                let n = args[1].clone();
                let bit = args[2].clone();

                // 1
                let _1 = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_unsigned(1, 64)),
                );


                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitInsert { target: op, source: bit, start: n, width: _1 }
                ))
            }

            // %bv -> %i
            "UInt0" | "unsigned" | "_builtin_unsigned" => {
                // just copy bits

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Cast {
                        kind: CastOperationKind::ZeroExtend,
                        typ: Type::u64(), // this is bad, bad, we're making rudder have really funky types in order to fix signed/unsigned division
                        value: args[0].clone(),
                    },
                ))
            }
            // %bv -> %i
            "SInt0" => {
                //                     void sail_signed(sail_int *rop, const lbits op)
                // {
                //   if (op.len == 0) {
                //     mpz_set_ui(*rop, 0);
                //   } else {
                //     mp_bitcnt_t sign_bit = op.len - 1;
                //     mpz_set(*rop, *op.bits);
                //     if (mpz_tstbit(*op.bits, sign_bit) != 0) {
                //       /* If sign bit is unset then we are done,
                //          otherwise clear sign_bit and subtract 2**sign_bit */
                //       mpz_set_ui(sail_lib_tmp1, 1);
                //       mpz_mul_2exp(sail_lib_tmp1, sail_lib_tmp1, sign_bit); /* 2**sign_bit */
                //       mpz_combit(*rop, sign_bit); /* clear sign_bit */
                //       mpz_sub(*rop, *rop, sail_lib_tmp1);
                //     }
                //   }
                // }
                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Cast {
                        kind: CastOperationKind::SignExtend,
                        typ: Type::s64(),
                        value: args[0].clone(),
                    },
                ))
            }

            // val ZeroExtend0 : (%bv, %i) -> %bv
            // val sail_zero_extend : (%bv, %i) -> %bv
            "ZeroExtend0" | "sail_zero_extend" | "sail_zzero_extend" => {
                let width = args[1].get(self.statement_arena());
                if let Statement::Constant(width) = width {
                    let width = match width {
                        Constant::UnsignedInteger { value, .. } => u32::try_from(*value).unwrap(),
                        Constant::SignedInteger { value, .. } => u32::try_from(*value).unwrap(),
                        _ => panic!(),
                    };
                    Some(build(
                        self.block,
                        self.block_arena_mut(),
                        Statement::Cast {
                            kind: CastOperationKind::ZeroExtend,
                            typ: (Type::new_primitive(PrimitiveType::UnsignedInteger(width))),
                            value: args[0].clone(),
                        },
                    ))
                } else {
                    Some(build(
                        self.block,
                        self.block_arena_mut(),
                        Statement::BitsCast {
                            kind: CastOperationKind::ZeroExtend,
                            typ: (Type::Bits),
                            value: args[0],
                            width: args[1],
                        },
                    ))
                }
            }

            // val SignExtend0 : (%bv, %i) -> %bv
            // val sail_sign_extend : (%bv, %i) -> %bv
            "SignExtend0" | "sail_sign_extend" => match (
                args[0]
                    .get(self.statement_arena())
                    .typ(self.statement_arena())
                    .unwrap()
                    .clone(),
                args[1].get(self.statement_arena()).clone(),
            ) {
                (
                    Type::Primitive(PrimitiveType::UnsignedInteger(_)),
                    Statement::Constant(Constant::SignedInteger { value: width, .. }),
                ) => Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Cast {
                        kind: CastOperationKind::SignExtend,
                        typ: Type::Primitive(PrimitiveType::UnsignedInteger(
                            u32::try_from(width).unwrap(),
                        )),
                        value: args[0],
                    },
                )),
                (Type::Bits, _) => Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitsCast {
                        kind: CastOperationKind::SignExtend,
                        typ: (Type::Bits),
                        value: args[0].clone(),
                        width: args[1].clone(),
                    },
                )),
                // used to fix SignExtend0(..., esize) in execute_FCVTZU_Z_P_Z_D2X. esize is
                // constant 64, and it's extending from a bv64 to a bv64?
                // todo: check that it's not doing any extra logic we need to replicate
                (Type::Primitive(PrimitiveType::UnsignedInteger(src_width)), _) => {
                    // target width is not a constant
                    let Some(expr) = expression else {
                        panic!("sign extend called with no destination")
                    };

                    let Expression::Identifier(ident) = expr else {
                        todo!()
                    };

                    let dest_typ = self
                        .fn_ctx()
                        .rudder_fn
                        .local_variables()
                        .iter()
                        .find(|sym| sym.name() == *ident)
                        .unwrap()
                        .typ();

                    match dest_typ {
                        Type::Primitive(PrimitiveType::UnsignedInteger(dest_width)) => {
                            match dest_width.cmp(&src_width) {
                                Ordering::Equal => Some(args[0]),
                                Ordering::Greater => Some(build(
                                    self.block,
                                    self.block_arena_mut(),
                                    Statement::Cast {
                                        kind: CastOperationKind::SignExtend,
                                        typ: dest_typ,
                                        value: args[0],
                                    },
                                )),
                                Ordering::Less => {
                                    panic!("truncation");
                                }
                            }
                        }
                        Type::Bits => Some(build(
                            self.block,
                            self.block_arena_mut(),
                            Statement::BitsCast {
                                kind: CastOperationKind::SignExtend,
                                typ: dest_typ,
                                value: args[0],
                                width: args[1],
                            },
                        )),
                        t => todo!("{t:?}"),
                    }
                }
                (typ, target_width) => todo!(
                    "sign extend {typ:?} {target_width:?} {:?} {:?}",
                    args[0],
                    args[1]
                ),
            },

            // val truncate : (%bv, %i) -> %bv
            "truncate" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::BitsCast {
                    kind: CastOperationKind::Truncate,
                    typ: (Type::Bits),
                    value: args[0].clone(),
                    width: args[1].clone(),
                },
            )),

            "sail_zeros" | "Zeros" | "sail_zzeros" => {
                let length = args[0].clone();

                let const_0 = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_unsigned(0, 64)),
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::CreateBits {
                        value: const_0,
                        width: length,
                    },
                ))
            }

            "sail_assert" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Assert {
                    condition: args[0].clone(),
                },
            )),

            // val bitvector_update : (%bv, %i, %bit) -> %bv
            "bitvector_update" => {
                let target = args[0];
                let i = args[1];

                assert_eq!(
                    args[2]
                        .get(self.statement_arena())
                        .typ(self.statement_arena()),
                    Some(Type::u1())
                );

                let bit = args[2];

                let const_1 = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_unsigned(1, 64)),
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitInsert {
                        target,
                        source: bit,
                        start: i,
                        width: const_1,
                    },
                ))
            }

            // val append_64 : (%bv, %bv64) -> %bv
            "append_64" => Some(self.generate_concat(args[0].clone(), args[1].clone())),

            "bitvector_concat" => Some(self.generate_concat(args[0].clone(), args[1].clone())),

            // val set_slice_int : (%i, %i, %i, %bv) -> %i
            "set_slice_int" => {
                // const sail_int len, const sail_int n, const sail_int start, const lbits slice
                let len = args[0].clone();
                let n = args[1].clone();
                let start = args[2].clone();

                // cast the slice from a %bv to a i128 for the i128 version of bit-insert
                let slice = args[3].clone();

                // destination[start..] = source[0..source.len()]
                // todo: check correctness and write some unit tests for this

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitInsert {
                        target: n,
                        source: slice,
                        start,
                        width: len,
                    },
                ))
            }

            //val get_slice_int : (%i, %i, %i) -> %bv
            "get_slice_int" => {
                let extract = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitExtract {
                        value: args[1].clone(),
                        start: args[2].clone(),
                        width: args[0].clone(),
                    },
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::CreateBits {
                        value: extract,
                        width: args[0].clone(),
                    },
                ))
            }

            // val set_slice_bits : (%i, %i, %bv, %i, %bv) -> %bv
            "set_slice_bits" => {
                // len, slen, x, pos, y
                let _len = args[0].clone();
                let slen = args[1].clone();
                let destination = args[2].clone();
                let start = args[3].clone();
                let source = args[4].clone();

                // destination[start..] = source[0..source.len()]
                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitInsert {
                        target: destination,
                        source,
                        start,
                        width: slen,
                    },
                ))
            }

            "update_subrange_bits" => {
                let destination = args[0].clone();
                let end = args[1].clone();
                let start = args[2].clone();
                let source = args[3].clone();

                let diff = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::Sub,
                        lhs: end,
                        rhs: start.clone(),
                    },
                );

                let const_1 = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_signed(1, 64)),
                );

                let source_length = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::Add,
                        lhs: diff,
                        rhs: const_1,
                    },
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitInsert {
                        target: destination,
                        source,
                        start,
                        width: source_length,
                    },
                ))
            }

            "replicate_bits" => Some(build(
                self.block,
                self.block_arena_mut(),
                // Statement::Call {
                //     target: REPLICATE_BITS_BOREALIS_INTERNAL.name(),
                //     args: vec![args[0].clone(), args[1].clone()],
                //     return_type: REPLICATE_BITS_BOREALIS_INTERNAL.return_type(),
                // },
                Statement::BitReplicate {
                    pattern: args[0].clone(),
                    count: args[1].clone(),
                },
            )),




            /* ### NON-BUILTIN FUNCTIONS BELOW THIS POINT ### */
            "unsigned_subrange" => {
                let xs = args[0].clone();
                let i = args[1].clone();
                let j = args[2].clone();

                // let diff = build(
                //     self.block,
                //     self.block_arena_mut(),
                //     Statement::BinaryOperation {
                //         kind: BinaryOperationKind::Sub,
                //         lhs: i,
                //         rhs: j.clone(),
                //     },
                // );

                // let const_1 = build(
                //     self.block,
                //     self.block_arena_mut(),
                //     Statement::Constant(Constant::new_signed(1, 64)),
                // );

                // let width =  build(
                //     self.block,
                //     self.block_arena_mut(),
                //     Statement::BinaryOperation {
                //         kind: BinaryOperationKind::Add,
                //         lhs: diff,
                //         rhs: const_1,
                //     },
                // );

                let extracted = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitExtract { value: xs, start: j, width: i }
                );

                let cast = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Cast { kind: CastOperationKind::ZeroExtend, typ: Type::s64(), value: extracted }
                );

                Some(cast)
            }

            "AddWithCarry" => {
                let x = args[0].clone();
                let y = args[1].clone();
                let carry_in = args[2].clone();

                let sum = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::TernaryOperation {
                        kind: TernaryOperationKind::AddWithCarry,
                        a: x,
                        b: y,
                        c: carry_in,
                    },
                );

                // nzcv bv4
                let flags = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::GetFlags {
                        operation: sum.clone(),
                    },
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::CreateTuple(vec![sum, flags]),
                ))
            }

            // "FixedToFP" => {
            //     let int_value = args[0].clone();
            //     let fbits = args[1].clone(); // 0?
            //     let is_unsigned = args[2].clone();
            //     let fpcr = args[3].clone();
            //     let rounding = args[4].clone();
            //     let float_size = args[5].clone();

            //     let zero =  build(
            //         self.block,
            //         self.block_arena_mut(),
            //         Statement::Constant(Constant::UnsignedInteger { value: 0, width: 64 }));

            //     Some(build(
            //         self.block,
            //         self.block_arena_mut(),
            //         Statement::CreateBits { value:zero , width: float_size }))
            // },

            // "FPToFixed" => {
            //      let float_value = args[0].clone();
            //     let fbits = args[1].clone(); // 0?
            //     let is_unsigned = args[2].clone();
            //     let fpcr = args[3].clone();
            //     let rounding = args[4].clone();
            //     let int_size = args[5].clone();

            //      Some(build(
            //         self.block,
            //         self.block_arena_mut(),
            //         Statement::Cast { kind: CastOperationKind::Convert, typ: Type::Primitive(()), value: () }))
            // },

            // todo: bad hack this really shouldn't be specialized, fix it
            "V_set"  => {
                let n = args[0].clone();
                let width = args[1].clone();
                let value = args[2].clone();

                let zero_extended = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Cast { kind: CastOperationKind::ZeroExtend, typ: Type::u128(), value: value });

                    let _256 =  build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::UnsignedInteger { value: 256, width: 64 }));

                    let z_offset = self.ctx().registers.get(&InternedString::from_static("_Z")).unwrap().offset;
                    let z =  build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::UnsignedInteger { value: z_offset, width: 64 }));


                let offset = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation { kind: BinaryOperationKind::Multiply, lhs: n, rhs: _256 });

                let offset = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation { kind: BinaryOperationKind::Add, lhs:z , rhs: offset });

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::WriteRegister { offset, value: zero_extended},
                ))
            }

            // /* To maintain correctness, borealis must only specialize on actual Sail compiler
            //  * builtins, specializing other functions means restricting compatibiliy on a
            //  * specific model, however memory access is the one exception to this, and must be
            //  * intercepted */

            // bits(64), CacheType
            "AArch64_MemZero" => {
                let address = args[0].clone();
                let value = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_unsigned(0, 8)),
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::WriteMemory { address, value },
                ))
            }

            // val Mem_read__2 : (%bv64, %i64, struct AccessDescriptor, %bool, %bool) -> %bv
            "Mem_read" | "Mem_read__1" | "Mem_read__2" => {
                let address = args[0].clone();
                let size = args[1].clone();
                //let _accdesc = args[2].clone();

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::ReadMemory { address, size },
                ))
            }

            "MemAtomic" => {
                let address = args[0].clone();
                let compare_operand = args[1].clone();
                let operand = args[2].clone();
                let modop = args[17].clone();

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::AtomicOperation { address, compare_operand, operand, operation_kind: modop },
                ))
            }

            // val Mem_set__2 : (%bv64, %i, struct AccessDescriptor, %bool, %bool, %bv) -> %unit
            "Mem_set" | "Mem_set__1" | "Mem_set__2" => {
                let address = args[0].clone();
                let _size = args[1].clone();
                //let _accdesc = args[2].clone();
                let value = args.last().cloned().unwrap();

                // assert-eq size == value.width

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::WriteMemory { address, value },
                ))
            }

            "__id" => Some(args[0].clone()),

            // ignore
            "append_str" | "__monomorphize" | "concat_str" => Some(args[0].clone()),

            // result of sail_mem_read always appears to ignore the value returned by `read_tag#`
            // (underscore in Ok((value, _))): match sail_mem_read(read_request(accdesc,
            // translation_info, size, desc.vaddress, desc.paddress.address)) {
            //     Ok((value, _)) => (CreatePhysMemRetStatus(Fault_None), value),
            //     Err(statuscode) => (CreatePhysMemRetStatus(statuscode), sail_zeros(8 * size))
            //   }
            "read_tag#" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Constant(Constant::new_unsigned(1, 1)),
            )),

            "write_tag#" => {
                let msg = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::String("write_tag panic".into())),
                );
                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Panic(msg),
                ))
            }

            "DecStr" | "bits_str" | "HexStr" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Constant(Constant::String(
                    "fix me in build_specialized_function".into(),
                )),
            )),

            // todo: remove me!
            "HaveBRBExt" | "HaveStatisticalProfiling" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Constant(Constant::new_unsigned(0, 1)),
            )),

            "HaveGCS" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Constant(Constant::new_unsigned(1, 1)),
            )),

            "__GetVerbosity" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Constant(Constant::new_unsigned(0, 64)),
            )),

            "get_cycle_count" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Constant(Constant::new_signed(0, 64)),
            )),

            // requires u256 internally :(
            "SHA256hash" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Constant(Constant::new_unsigned(0, 256)),
            )),

            // val putchar : (%i) -> %unit
            "putchar" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Panic(args[0].clone()),
            )),

            "AArch64_DC"
            | "execute_aarch64_instrs_system_barriers_dmb"
            | "execute_aarch64_instrs_system_barriers_dsb"
            | "execute_aarch64_instrs_system_barriers_isb"
            | "sail_return_exception"
            | "sail_branch_announce"
            | "prerr_bits"
            | "prerr_int"
            | "sail_cache_op"
            | "sail_barrier"
            | "__WakeupRequest"
            | "print"
            | "print_endline"
            | "check_cycle_count"
            | "sail_take_exception"
            | "CheckSPAlignment"
            | "AArch64_SetExclusiveMonitors"
            | "UsingAArch32"
            | "ELUsingAArch32"
            | "EffectiveTBI"
            | "GCSPCREnabled"
            | "EndOfInstruction"
            | "CheckFPAdvSIMDEnabled64" // todo: re-enable this and make sure it's optimized away rather than hard-coded
            =>
            // todo: don't replace with constant, delete
            {
                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_unsigned(0, 1)),
                ))
            }

            // replace with "true"
            "AArch64_ExclusiveMonitorsPass" | "HaveAArch64" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Constant(Constant::new_unsigned(1, 1)),
            )),

            "PhysicalCountInt" => {
                let offset = self
                    .ctx()
                    .registers
                    .get(&InternedString::from_static("PhysicalCount"))
                    .unwrap()
                    .offset;

                let offset_const = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_unsigned(offset, 32)),
                );

                let read_reg = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::ReadRegister {
                        typ: Type::Primitive(PrimitiveType::SignedInteger(64)),
                        offset: offset_const,
                    },
                );

                Some(build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Cast {
                        kind: CastOperationKind::Reinterpret,
                        typ: Type::u64(),
                        value: read_reg,
                    },
                ))
            }

            "CurrentVL_read" => Some(build(
                self.block,
                self.block_arena_mut(),
                Statement::Constant(Constant::new_signed(128, 64)),
            )),
            _ => None,
        }
    }

    // fn build_union_constructor(
    //     &mut self,
    //     name: InternedString,
    //     args: &[Statement],
    // ) -> Option<Statement> {
    //     self.ctx()
    //         .unions
    //         .values()
    //         .find(|(_, variants)| variants.contains_key(&name))
    //         .map(|(typ, _)| typ)
    //         .cloned()
    //         .map(|typ| {
    //             build(   self.block,
    // self.block_arena_mut(),Statement::CreateEnum {                 typ,
    //                 variant: name,
    //                 value: args[0].clone(),
    //             })
    //         })
    // }

    /// Generates rudder for a writing a statement to a boom::Expression
    fn build_expression_write(&mut self, target: &boom::Expression, source: Ref<Statement>) {
        let boom::Expression::Identifier(ident) = target else {
            panic!(
                "got non ident expression: {target:?} in {:?}",
                self.fn_ctx().rudder_fn.name()
            );
        };

        match self.fn_ctx_mut().rudder_fn.get_local_variable(*ident) {
            Some(symbol) => {
                //   let (_, outer_type) = fields_to_indices(&self.ctx().structs, symbol.typ(),
                // fields);

                let value = cast(self.block, self.block_arena_mut(), source, symbol.typ());

                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::WriteVariable { symbol, value },
                );
            }
            None => {
                if ident.as_ref() == "_PC" {
                    let cast = cast(self.block, self.block_arena_mut(), source, Type::u64());
                    build(
                        self.block,
                        self.block_arena_mut(),
                        Statement::WritePc { value: cast },
                    );
                } else {
                    //register lookup
                    let Some(RegisterDescriptor {
                        typ: register_type,
                        offset: register_offset,
                        ..
                    }) = self.ctx().registers.get(ident).cloned()
                    else {
                        panic!("wtf is {ident} in {}", self.fn_ctx().rudder_fn.name());
                    };

                    // cast to outermost type
                    let cast = cast(self.block, self.block_arena_mut(), source, register_type);

                    let offset = build(
                        self.block,
                        self.block_arena_mut(),
                        Statement::Constant(Constant::new_unsigned(
                            u64::try_from(register_offset).unwrap(),
                            32,
                        )),
                    );

                    build(
                        self.block,
                        self.block_arena_mut(),
                        Statement::WriteRegister {
                            offset,
                            value: cast,
                        },
                    );
                }
            }
        }
    }

    /// Last statement returned is the value
    fn build_value(&mut self, boom_value: Shared<boom::Value>) -> Ref<Statement> {
        match &*boom_value.get() {
            boom::Value::Identifier(ident) => {
                // local variable
                if let Some(symbol) = self.fn_ctx_mut().rudder_fn.get_local_variable(*ident) {
                    return build(
                        self.block,
                        self.block_arena_mut(),
                        Statement::ReadVariable { symbol },
                    );
                }

                // parameter
                if let Some(symbol) = self.fn_ctx_mut().rudder_fn.get_parameter(*ident) {
                    return build(
                        self.block,
                        self.block_arena_mut(),
                        Statement::ReadVariable { symbol },
                    );
                }

                // register
                if let Some(RegisterDescriptor {
                    typ,
                    offset: register_offset,
                    ..
                }) = self.ctx().registers.get(ident).cloned()
                {
                    let offset = build(
                        self.block,
                        self.block_arena_mut(),
                        Statement::Constant(Constant::new_unsigned(
                            u64::try_from(register_offset).unwrap(),
                            32,
                        )),
                    );

                    return build(
                        self.block,
                        self.block_arena_mut(),
                        Statement::ReadRegister {
                            typ: typ.clone(),
                            offset,
                        },
                    );
                }

                panic!(
                    "unknown ident: {:?} in value {:?} in block {:?} in {:?}",
                    ident,
                    boom_value,
                    self.block,
                    self.fn_ctx().rudder_fn.name()
                );
            }
            boom::Value::Literal(literal) => self.build_literal(&literal.get()),
            boom::Value::Operation(op) => self.build_operation(op),
            boom::Value::Tuple(values) => {
                let values = values.iter().map(|v| self.build_value(v.clone())).collect();
                return build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::CreateTuple(values),
                );
            }
            boom::Value::Struct { name, .. } => {
                let c = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::String(
                        format!("boom attempted to build a struct {name:?} here").into(),
                    )),
                );
                return build(self.block, self.block_arena_mut(), Statement::Panic(c));
                // todo: do whatever to enable this panic
                //      panic!("got struct {name} {fields:?} but structs should
                // have been removed in boom")
            }
            boom::Value::Field { .. } => panic!(
                "fields should have already been flattened: {:?}",
                self.function_build_context.rudder_fn.name()
            ),
            boom::Value::CtorKind {
                value, identifier, ..
            } => {
                let target_tag = self
                    .ctx()
                    .unions
                    .get(identifier)
                    .unwrap_or_else(|| panic!("failed to find tag value for {identifier:?}"))
                    .1;

                let target_tag = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_signed(i64::from(target_tag), 64)),
                );

                let read_tag = {
                    let boom::Value::Identifier(union_root) = &*value.get() else {
                        panic!()
                    };
                    build(
                        self.block,
                        self.block_arena_mut(),
                        Statement::ReadVariable {
                            symbol: Symbol::new(
                                destruct_composites::union_tag_ident(*union_root),
                                Type::Primitive(PrimitiveType::SignedInteger(32)),
                            ),
                        },
                    )
                };

                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::CompareEqual,
                        lhs: target_tag,
                        rhs: read_tag,
                    },
                )
            }
            boom::Value::CtorUnwrap {
                value, identifier, ..
            } => {
                let boom::Value::Identifier(union_root) = &*value.get() else {
                    panic!()
                };
                let typ = self
                    .ctx()
                    .unions
                    .get(identifier)
                    .unwrap_or_else(|| panic!("failed to find tag value for {identifier:?}"))
                    .0
                    .clone()
                    .unwrap();
                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::ReadVariable {
                        symbol: Symbol::new(
                            destruct_composites::union_value_ident(*union_root, *identifier),
                            typ,
                        ),
                    },
                )
            }
            boom::Value::VectorAccess { value, index } => {
                let vector = self.build_value(value.clone());
                let index = self.build_value(index.clone());
                return build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::ReadElement { vector, index },
                );
            }
            boom::Value::VectorMutate {
                vector,
                element,
                index,
            } => {
                let vector = self.build_value(vector.clone());
                let index = self.build_value(index.clone());
                let element = self.build_value(element.clone());
                return build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::AssignElement {
                        vector,
                        value: element,
                        index,
                    },
                );
            }
            boom::Value::Real {
                numerator,
                denominator,
            } => {
                let numerator = self.build_value(numerator.clone());
                let denominator = self.build_value(denominator.clone());

                return build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::CreateReal {
                        numerator,
                        denominator,
                    },
                );
            }
        }
    }

    fn build_literal(&mut self, literal: &boom::Literal) -> Ref<Statement> {
        let value = build_constant_value(literal);

        let kind = match literal {
            boom::Literal::Int(_)
            | boom::Literal::Bits(_)
            | boom::Literal::Bit(_)
            | boom::Literal::Bool(_)
            | boom::Literal::String(_) => Statement::Constant(value),
            boom::Literal::Unit => unreachable!("units removed in boom pass"),
            boom::Literal::Reference(_) => todo!(),
            boom::Literal::Undefined => todo!(),
            boom::Literal::Vector(vec) => Statement::Constant(Constant::Vector(
                vec.iter()
                    .map(|l| build_constant_value(&*l.get()))
                    .collect(),
            )),
        };

        build(self.block, self.block_arena_mut(), kind)
    }

    fn build_operation(&mut self, op: &boom::Operation) -> Ref<Statement> {
        match op {
            boom::Operation::Not(value) => {
                let value = self.build_value(value.clone());
                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::UnaryOperation {
                        kind: UnaryOperationKind::Not,
                        value,
                    },
                )
            }
            boom::Operation::Complement(value) => {
                let value = self.build_value(value.clone());
                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::UnaryOperation {
                        kind: UnaryOperationKind::Complement,
                        value,
                    },
                )
            }
            boom::Operation::Cast(value, typ) => {
                let target_type = self.ctx().resolve_type(typ.clone());
                let value = self.build_value(value.clone());

                let source_type = value
                    .get(self.statement_arena())
                    .typ(self.statement_arena())
                    .unwrap();

                let kind = match source_type {
                    Type::Vector { .. } | Type::String => {
                        panic!("cast on non-primitive type")
                    }
                    Type::Primitive(_) => {
                        match source_type.width_bits().cmp(&target_type.width_bits()) {
                            Ordering::Less => CastOperationKind::ZeroExtend,
                            Ordering::Greater => CastOperationKind::Truncate,
                            Ordering::Equal => CastOperationKind::Reinterpret,
                        }
                    }
                    Type::Bits => todo!(),
                    Type::Int => todo!(),
                    Type::Tuple(_) => todo!(),
                    Type::Real => panic!("absolutely not"),
                };

                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Cast {
                        kind,
                        typ: target_type,
                        value,
                    },
                )
            }

            boom::Operation::LeftShift(value, amount)
            | boom::Operation::RightShift(value, amount)
            | boom::Operation::RotateRight(value, amount)
            | boom::Operation::RotateLeft(value, amount) => {
                let value = self.build_value(value.clone());
                let amount = self.build_value(amount.clone());

                let kind = match op {
                    boom::Operation::LeftShift(_, _) => ShiftOperationKind::LogicalShiftLeft,
                    boom::Operation::RightShift(_, _) => {
                        // todo figure out if logical or arithmetic
                        ShiftOperationKind::LogicalShiftRight
                    }
                    boom::Operation::RotateRight(_, _) => ShiftOperationKind::RotateRight,
                    boom::Operation::RotateLeft(_, _) => ShiftOperationKind::RotateLeft,

                    _ => unreachable!(),
                };

                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::ShiftOperation {
                        kind,
                        value,
                        amount,
                    },
                )
            }

            boom::Operation::Equal(left, right)
            | boom::Operation::NotEqual(left, right)
            | boom::Operation::LessThan(left, right)
            | boom::Operation::LessThanOrEqual(left, right)
            | boom::Operation::GreaterThan(left, right)
            | boom::Operation::GreaterThanOrEqual(left, right)
            | boom::Operation::Subtract(left, right)
            | boom::Operation::Add(left, right)
            | boom::Operation::Or(left, right)
            | boom::Operation::Multiply(left, right)
            | boom::Operation::And(left, right)
            | boom::Operation::Xor(left, right)
            | boom::Operation::Divide(left, right) => {
                let mut lhs = self.build_value(left.clone());
                let mut rhs = self.build_value(right.clone());

                let arena = self.statement_arena();

                let left_type = lhs.get(arena).typ(arena).unwrap();
                let right_type = rhs.get(arena).typ(arena).unwrap();

                if left_type != right_type {
                    // need to insert casts
                    let destination_type = if left_type.width_bits() > right_type.width_bits() {
                        left_type
                    } else {
                        right_type
                    };

                    lhs = cast(
                        self.block,
                        self.block_arena_mut(),
                        lhs.clone(),
                        destination_type.clone(),
                    );
                    rhs = cast(
                        self.block,
                        self.block_arena_mut(),
                        rhs.clone(),
                        destination_type,
                    );
                }

                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: match op {
                            boom::Operation::Equal(_, _) => BinaryOperationKind::CompareEqual,
                            boom::Operation::NotEqual(_, _) => BinaryOperationKind::CompareNotEqual,
                            boom::Operation::LessThan(_, _) => BinaryOperationKind::CompareLessThan,
                            boom::Operation::LessThanOrEqual(_, _) => {
                                BinaryOperationKind::CompareLessThanOrEqual
                            }
                            boom::Operation::GreaterThan(_, _) => {
                                BinaryOperationKind::CompareGreaterThan
                            }
                            boom::Operation::GreaterThanOrEqual(_, _) => {
                                BinaryOperationKind::CompareGreaterThanOrEqual
                            }
                            boom::Operation::Subtract(_, _) => BinaryOperationKind::Sub,
                            boom::Operation::Add(_, _) => BinaryOperationKind::Add,
                            boom::Operation::Or(_, _) => BinaryOperationKind::Or,
                            boom::Operation::Multiply(_, _) => BinaryOperationKind::Multiply,
                            boom::Operation::And(_, _) => BinaryOperationKind::And,
                            boom::Operation::Xor(_, _) => BinaryOperationKind::Xor,
                            boom::Operation::Divide(_, _) => BinaryOperationKind::Divide,

                            _ => unreachable!(),
                        },
                        lhs,
                        rhs,
                    },
                )
            }
        }
    }

    fn generate_concat(&mut self, left: Ref<Statement>, right: Ref<Statement>) -> Ref<Statement> {
        let arena = self.statement_arena();

        // todo: (zero extend original value || create new bits with runtime length)
        // then bitinsert
        match (
            left.get(arena).typ(arena).unwrap(),
            right.get(arena).typ(arena).unwrap(),
        ) {
            (
                Type::Primitive(PrimitiveType::UnsignedInteger(left_width)),
                Type::Primitive(PrimitiveType::UnsignedInteger(right_width)),
            ) => {
                let target_width = left_width + right_width;

                let right_cast = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Cast {
                        kind: CastOperationKind::ZeroExtend,
                        typ: Type::Primitive(PrimitiveType::UnsignedInteger(target_width)),
                        value: right,
                    },
                );

                let left_width_constant = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_unsigned(
                        u64::try_from(left_width).unwrap(),
                        16,
                    )),
                );

                let right_width_constant = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::Constant(Constant::new_unsigned(
                        u64::try_from(right_width).unwrap(),
                        16,
                    )),
                );

                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitInsert {
                        target: right_cast,
                        source: left,
                        start: right_width_constant,
                        width: left_width_constant,
                    },
                )
            }
            (
                Type::Bits | Type::Primitive(PrimitiveType::UnsignedInteger(_)),
                Type::Bits | Type::Primitive(PrimitiveType::UnsignedInteger(_)),
            ) => {
                let l_length = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::SizeOf { value: left },
                );

                let r_length = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::SizeOf { value: right },
                );

                let sum_length = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::Add,
                        lhs: l_length,
                        rhs: r_length,
                    },
                );

                let tmp = build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::CreateBits {
                        value: right,
                        width: sum_length,
                    },
                );

                build(
                    self.block,
                    self.block_arena_mut(),
                    Statement::BitInsert {
                        target: tmp,
                        source: left,
                        start: r_length,
                        width: l_length,
                    },
                )
            }

            (a, b) => panic!("todo concat for {a:?} {b:?}"),
        }
    }
}

fn build_constant_value(literal: &boom::Literal) -> Constant {
    match literal {
        boom::Literal::Int(i) => {
            let value = i.try_into().unwrap_or_else(|_| {
                log::error!("failed to convert {i} to i64");
                i64::MAX
            });
            Constant::new_signed(value, 64)
        }
        boom::Literal::Bits(bits) => Constant::new_unsigned(
            bits_to_int(bits).try_into().unwrap(),
            u32::try_from(bits.len()).unwrap(),
        ),
        boom::Literal::Bit(bit) => Constant::new_unsigned(bit.value().try_into().unwrap(), 1),

        boom::Literal::Bool(b) => Constant::new_unsigned(if *b { 1 } else { 0 }, 1),

        boom::Literal::String(str) => Constant::String(*str),

        boom::Literal::Unit => unreachable!("units removed in boom pass"),
        boom::Literal::Reference(_) => todo!(),
        boom::Literal::Undefined => todo!(),
        boom::Literal::Vector(vec) => Constant::Vector(
            vec.iter()
                .map(|l| build_constant_value(&*l.get()))
                .collect(),
        ),
    }
}
