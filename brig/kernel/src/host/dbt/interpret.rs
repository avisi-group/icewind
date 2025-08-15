use {
    crate::host::dbt::{bit_extract, bit_insert, register_file::RegisterFile},
    alloc::vec::Vec,
    common::{
        arena::Ref,
        hashmap::HashMap,
        intern::InternedString,
        mask::mask,
        rudder::{
            Model,
            block::Block,
            constant::Constant,
            statement::{
                BinaryOperationKind, CastOperationKind, ShiftOperationKind, Statement,
                TernaryOperationKind, UnaryOperationKind,
            },
            types::{PrimitiveType, Type},
        },
    },
    core::{
        borrow::Borrow,
        cmp::{Ordering, max},
        ops::{Add, BitAnd, BitOr, Div, Mul, Sub},
        panic, usize,
    },
};

pub fn interpret(
    model: &Model,
    function_name: &str,
    arguments: &[Value],
    register_file: &RegisterFile,
) -> Option<Value> {
    log::debug!("interpreting {function_name}");
    let function_name = InternedString::from(function_name);
    let function = model.functions().get(&function_name).unwrap();

    let mut interpreter = Interpreter::new(model, function_name, register_file);

    // insert arguments
    interpreter.locals.extend(
        function
            .parameters()
            .into_iter()
            .zip(arguments.iter())
            .map(|(symbol, value)| (symbol.name(), value.clone())),
    );

    let mut current_block = function.entry_block();
    loop {
        match interpreter.interpret_block(current_block) {
            BlockResult::NextBlock(next) => current_block = next,
            BlockResult::ReturnValue(value) => return value,
        }
    }
}

struct Interpreter<'f, 'r> {
    model: &'f Model,
    function_name: InternedString,
    // local variables
    locals: HashMap<InternedString, Value>,
    // value of previously evaluated statements
    statement_values: HashMap<Ref<Statement>, Value>,
    register_file: &'r RegisterFile,
    // nzcv
    flags: u8,
}

impl<'f, 'r> Interpreter<'f, 'r> {
    fn new(
        model: &'f Model,
        function_name: InternedString,
        register_file: &'r RegisterFile,
    ) -> Self {
        Self {
            model,
            function_name,
            locals: HashMap::default(),
            statement_values: HashMap::default(),
            register_file,
            flags: 0,
        }
    }

    fn resolve<R: Borrow<Ref<Statement>>>(&self, statement_ref: R) -> Value {
        self.statement_values
            .get(statement_ref.borrow())
            .unwrap_or_else(|| {
                panic!(
                    "failed to resolve {:?} in {:?}\nstatement_values: {:?}",
                    statement_ref.borrow(),
                    self.function_name,
                    self.statement_values,
                )
            })
            .clone()
    }

    fn resolve_u64<R: Borrow<Ref<Statement>>>(&self, statement_ref: R) -> u64 {
        match self.resolve(statement_ref) {
            Value::UnsignedInteger { value: u, width: _ } => u,
            Value::SignedInteger { value: i, width: _ } => i
                .try_into()
                .unwrap_or_else(|_| panic!("cannot resolve {i} as u64")),
            _ => panic!(),
        }
    }

    fn interpret_block(&mut self, block_ref: Ref<Block>) -> BlockResult {
        log::trace!("{}: block {block_ref:?}", self.function_name);
        self.statement_values.clear();
        let block = block_ref.get(
            self.model
                .functions()
                .get(&self.function_name)
                .unwrap()
                .arena(),
        );
        for statement_ref in block.statements() {
            let statement = statement_ref.get(block.arena());
            let value = match statement {
                Statement::Constant(value) => Some(Value::from_constant(value)),
                Statement::ReadVariable { symbol } => Some(
                    self.locals
                        .get(&symbol.name())
                        .unwrap_or_else(|| panic!("no local found for {symbol:?}"))
                        .clone(),
                ),
                Statement::ReadRegister { typ, offset } => {
                    let offset = usize::try_from(self.resolve_u64(offset)).unwrap();

                    Some(self.read_register(typ, offset))
                }
                Statement::ReadMemory { .. } => todo!(),
                Statement::ReadPc => todo!(),
                Statement::GetFlags { operation: _ } => {
                    // todo: technically should get the last statement or
                    // something
                    Some(Value::UnsignedInteger {
                        value: u64::from(self.flags),
                        width: 4,
                    })
                }
                Statement::UnaryOperation { kind, value } => {
                    let value = self.resolve(value);

                    match kind {
                        UnaryOperationKind::Not => {
                            let Value::UnsignedInteger { value, .. } = value else {
                                panic!()
                            };

                            Some(Value::UnsignedInteger {
                                value: (value == 0) as u64,
                                width: 1,
                            })
                        }
                        UnaryOperationKind::Complement => {
                            let Value::UnsignedInteger { value, width } = value else {
                                todo!()
                            };

                            Some(Value::UnsignedInteger {
                                value: !value,
                                width,
                            })
                        }
                        UnaryOperationKind::Negate => {
                            let Value::SignedInteger { value, width } = value else {
                                todo!()
                            };

                            Some(Value::SignedInteger {
                                value: -value,
                                width,
                            })
                        }
                        _ => todo!("{kind:?} {value:?}"),
                    }
                }
                Statement::BinaryOperation { kind, lhs, rhs } => {
                    let left = self.resolve(lhs);
                    let right = self.resolve(rhs);

                    Some(match kind {
                        BinaryOperationKind::CompareEqual => Value::UnsignedInteger {
                            value: (left == right) as u64,
                            width: 1,
                        },
                        BinaryOperationKind::CompareNotEqual => Value::UnsignedInteger {
                            value: (left != right) as u64,
                            width: 1,
                        },
                        BinaryOperationKind::CompareLessThan => Value::UnsignedInteger {
                            value: (left < right) as u64,
                            width: 1,
                        },
                        BinaryOperationKind::CompareLessThanOrEqual => Value::UnsignedInteger {
                            value: (left <= right) as u64,
                            width: 1,
                        },
                        BinaryOperationKind::CompareGreaterThan => Value::UnsignedInteger {
                            value: (left > right) as u64,
                            width: 1,
                        },
                        BinaryOperationKind::CompareGreaterThanOrEqual => Value::UnsignedInteger {
                            value: (left >= right) as u64,
                            width: 1,
                        },
                        BinaryOperationKind::Sub => left - right,
                        BinaryOperationKind::Add => left + right,
                        BinaryOperationKind::Multiply => left * right,
                        BinaryOperationKind::Or => left | right,
                        BinaryOperationKind::And => left & right,
                        BinaryOperationKind::Divide => left / right,
                        _ => todo!("{kind:?}"),
                    })
                }
                Statement::TernaryOperation { kind, a, b, c } => match kind {
                    TernaryOperationKind::AddWithCarry => {
                        let Value::UnsignedInteger { width, value: a } = self.resolve(a) else {
                            panic!()
                        };
                        assert_eq!(width, 64);
                        let Value::UnsignedInteger { width, value: b } = self.resolve(b) else {
                            panic!()
                        };
                        assert_eq!(width, 64);
                        let Value::UnsignedInteger { width, value: c } = self.resolve(c) else {
                            panic!()
                        };
                        assert_eq!(width, 1);

                        // function AddWithCarry (x, y, carry_in) = {
                        //     let 'unsigned_sum = UInt(x) + UInt(y) + UInt(carry_in);
                        //     let 'signed_sum = SInt(x) + SInt(y) + UInt(carry_in);
                        //     let result : bits('N) = unsigned_sum['N - 1 .. 0];
                        //     let n : bits(1) = [result['N - 1]];
                        //     let z : bits(1) = if IsZero(result) then 0b1 else 0b0;
                        //     let c : bits(1) = if UInt(result) == unsigned_sum then 0b0 else 0b1;
                        //     let v : bits(1) = if SInt(result) == signed_sum then 0b0 else 0b1;
                        //     return((result, ((n @ z) @ c) @ v))
                        // }

                        // not correct for lengths other than 64
                        let unsigned_sum = u128::from(a) + u128::from(b) + u128::from(c);
                        let signed_sum =
                            i128::from(a as i64) + i128::from(b as i64) + i128::from(c as i64);
                        let u_result = unsigned_sum as u64;
                        let i_result = unsigned_sum as i64;

                        let n = u_result >> 63;
                        let z = (u_result == 0) as u64;
                        let c = (u128::from(u_result) != unsigned_sum) as u64;
                        let v = (i128::from(i_result) != signed_sum) as u64;

                        self.flags = u8::try_from(n << 3 | z << 2 | c << 1 | v).unwrap();

                        Some(Value::UnsignedInteger {
                            value: u_result,
                            width: 64,
                        })
                    }
                },

                Statement::ShiftOperation {
                    kind,
                    value,
                    amount,
                } => {
                    let amount = self.resolve_u64(amount);

                    let value = self.resolve(value);

                    match (kind, &value) {
                        (
                            ShiftOperationKind::LogicalShiftLeft,
                            Value::UnsignedInteger { value, width },
                        ) => {
                            let (value, did_overflow) =
                                value.overflowing_shl(u32::try_from(amount).unwrap());

                            if did_overflow {
                                log::trace!("overflowed during lsl of {value} by {amount}");
                            }

                            Some(Value::UnsignedInteger {
                                value: value & mask(*width),
                                width: *width,
                            })
                        }
                        (
                            ShiftOperationKind::LogicalShiftRight,
                            Value::UnsignedInteger { value, width },
                        ) => {
                            let (value, did_overflow) =
                                value.overflowing_shr(u32::try_from(amount).unwrap());

                            if did_overflow {
                                log::trace!("overflowed during lsl of {value} by {amount}");
                            }

                            Some(Value::UnsignedInteger {
                                value: value & mask(*width),
                                width: *width,
                            })
                        }
                        (
                            ShiftOperationKind::LogicalShiftLeft,
                            Value::SignedInteger { value, width },
                        ) => Some(Value::SignedInteger {
                            value: value << amount,
                            width: *width,
                        }),
                        _ => todo!("{value:?} {kind:?} by {amount}"),
                    }
                }
                Statement::Call { target, args, .. } => {
                    log::trace!(
                        "{}: block {block_ref:?}: call {target:?}",
                        self.function_name
                    );

                    interpret(
                        &self.model,
                        target.as_ref(),
                        &args.iter().map(|a| self.resolve(a)).collect::<Vec<_>>(),
                        self.register_file,
                    )
                }
                Statement::Cast {
                    kind,
                    typ: dest_typ,
                    value,
                } => {
                    let source_typ = value.get(block.arena()).typ(block.arena()).unwrap();
                    let value = self.resolve(value);

                    match (&kind, &dest_typ, &value) {
                        (
                            CastOperationKind::SignExtend,
                            Type::Primitive(PrimitiveType::SignedInteger(width)),
                            Value::SignedInteger { value, .. },
                        ) => Some(Value::SignedInteger {
                            value: sign_extend(*value, source_typ.width_bits(), *width),
                            width: *width,
                        }),
                        (
                            CastOperationKind::Truncate,
                            Type::Primitive(PrimitiveType::UnsignedInteger(width)),
                            Value::UnsignedInteger { value, .. },
                        ) => Some(Value::UnsignedInteger {
                            value: value & mask(*width),
                            width: *width,
                        }),
                        (
                            CastOperationKind::Truncate,
                            Type::Primitive(PrimitiveType::SignedInteger(width)),
                            Value::SignedInteger { value, .. },
                        ) => Some(Value::SignedInteger {
                            value: ((*value as u64) & mask(*width)) as i64,
                            width: *width,
                        }),
                        (
                            CastOperationKind::Reinterpret,
                            Type::Primitive(PrimitiveType::SignedInteger(width)),
                            Value::UnsignedInteger { value, .. },
                        ) => Some(Value::SignedInteger {
                            value: i64::try_from(*value & mask(*width)).unwrap(),
                            width: *width,
                        }),
                        (
                            CastOperationKind::Reinterpret,
                            Type::Primitive(PrimitiveType::SignedInteger(width)),
                            Value::SignedInteger { value, .. },
                        ) => Some(Value::SignedInteger {
                            value: i64::try_from(*value as u64 & mask(*width)).unwrap(),
                            width: *width,
                        }),
                        (
                            CastOperationKind::ZeroExtend,
                            Type::Bits,
                            Value::UnsignedInteger { value, width },
                        ) => Some(Value::UnsignedInteger {
                            value: *value,
                            width: *width,
                        }),
                        (
                            CastOperationKind::ZeroExtend,
                            Type::Primitive(PrimitiveType::UnsignedInteger(width)),
                            Value::UnsignedInteger { value, .. },
                        ) => Some(Value::UnsignedInteger {
                            value: *value,
                            width: *width,
                        }),
                        (
                            CastOperationKind::Reinterpret,
                            Type::Primitive(PrimitiveType::UnsignedInteger(target_width)),
                            Value::UnsignedInteger { value, width },
                        ) => {
                            if target_width == width {
                                Some(Value::UnsignedInteger {
                                    value: *value,
                                    width: *width,
                                })
                            } else {
                                todo!("{target_width} != {width}")
                            }
                        }
                        (
                            CastOperationKind::ZeroExtend,
                            Type::Primitive(PrimitiveType::SignedInteger(width)),
                            Value::UnsignedInteger { value, .. },
                        ) => Some(Value::SignedInteger {
                            value: i64::try_from(*value).unwrap(),
                            width: *width,
                        }),
                        (
                            CastOperationKind::Truncate,
                            Type::Primitive(PrimitiveType::UnsignedInteger(width)),
                            Value::SignedInteger { value, .. },
                        ) => Some(Value::UnsignedInteger {
                            value: u64::try_from(*value).unwrap() & mask(*width),
                            width: *width,
                        }),
                        (CastOperationKind::Convert, Type::Bits, Value::UnsignedInteger { .. }) => {
                            Some(value)
                        }
                        (k, t, v) => todo!("cast type: {k:?}, to: {t:?}, from: {v:?}"),
                    }
                }
                Statement::BitsCast {
                    kind,
                    typ,
                    value,
                    width,
                } => {
                    let value = self.resolve(value);
                    let target_width = u32::try_from(self.resolve_u64(width)).unwrap();
                    match (kind, typ, &value) {
                        (
                            CastOperationKind::ZeroExtend,
                            Type::Bits,
                            Value::UnsignedInteger { value, width },
                        ) => {
                            if target_width > *width {
                                Some(Value::UnsignedInteger {
                                    value: value & mask(target_width),
                                    width: target_width,
                                })
                            } else {
                                panic!();
                            }
                        }
                        (
                            CastOperationKind::SignExtend,
                            Type::Bits,
                            Value::UnsignedInteger { value, width },
                        ) => {
                            if target_width > *width {
                                Some(Value::UnsignedInteger {
                                    value: value & mask(target_width),
                                    width: target_width,
                                })
                            } else {
                                panic!();
                            }
                        }
                        _ => todo!("{kind:?} {typ:?} {value:?} {width}"),
                    }
                }
                Statement::Select {
                    condition,
                    true_value,
                    false_value,
                } => {
                    let condition = self.resolve_u64(condition);

                    Some(self.resolve(if condition != 0 {
                        true_value
                    } else {
                        false_value
                    }))
                }
                Statement::BitExtract {
                    value,
                    start,
                    width,
                } => {
                    let value = self.resolve(value);
                    let start = self.resolve_u64(start);
                    let width = self.resolve_u64(width);

                    Some(match value {
                        Value::UnsignedInteger { value, .. } => Value::UnsignedInteger {
                            value: bit_extract(value, start, width),
                            width: u32::try_from(width).unwrap(),
                        },
                        // todo: test/verify this
                        Value::SignedInteger { value, .. } => Value::SignedInteger {
                            value: bit_extract(value as u64, start, width) as i64,
                            width: u32::try_from(width).unwrap(),
                        },
                        _ => todo!("{value:?}"),
                    })
                }
                Statement::BitInsert {
                    target,
                    source,
                    start,
                    width,
                } => {
                    let Value::UnsignedInteger {
                        value: target,
                        width: target_width,
                    } = self.resolve(target)
                    else {
                        panic!()
                    };

                    let source = self.resolve_u64(source);
                    let start = self.resolve_u64(start);
                    let width = self.resolve_u64(width);
                    Some(Value::UnsignedInteger {
                        value: bit_insert(target, source, start, width),
                        width: target_width,
                    })
                }
                Statement::BitReplicate { .. } => {
                    todo!()
                }

                Statement::ReadElement { .. } => todo!(),
                Statement::AssignElement {
                    vector,
                    value,
                    index,
                } => {
                    let vector = self.resolve(vector);
                    let value = self.resolve(value);
                    let index = usize::try_from(self.resolve_u64(index)).unwrap();

                    let Value::Vector(mut vec) = vector else {
                        panic!()
                    };

                    vec[index] = value;

                    Some(Value::Vector(vec))
                }
                Statement::CreateBits { value, width } => {
                    let value = self.resolve_u64(value);
                    let width = self.resolve_u64(width);

                    Some(Value::UnsignedInteger {
                        value,
                        width: u32::try_from(width).unwrap(),
                    })
                }
                Statement::SizeOf { value } => {
                    let value = self.resolve(value);
                    match value {
                        Value::UnsignedInteger { width, .. } => Some(Value::UnsignedInteger {
                            value: u64::from(width),
                            width: 16,
                        }),
                        _ => todo!("size-of {value:?}"),
                    }
                }
                Statement::MatchesUnion { .. } => todo!(),
                Statement::UnwrapUnion { .. } => todo!(),
                Statement::CreateTuple(vec) => {
                    Some(Value::Tuple(vec.iter().map(|s| self.resolve(s)).collect()))
                }
                Statement::TupleAccess { index, source } => {
                    let Value::Tuple(values) = self.resolve(source) else {
                        panic!(
                            "attempted tuple-access {index} of {:?}",
                            self.resolve(source)
                        )
                    };

                    Some(values[*index].clone())
                }

                Statement::WriteVariable { symbol, value } => {
                    self.locals.insert(symbol.name(), self.resolve(value));
                    None
                }
                Statement::WriteRegister { offset, value } => {
                    let offset = usize::try_from(self.resolve_u64(offset)).unwrap();

                    self.write_register(self.resolve(value), offset);

                    None
                }
                Statement::WriteMemory { .. } => todo!(),
                Statement::WritePc { value } => {
                    self.register_file.write_raw(
                        self.model.reg_offset(InternedString::from_static("_PC")) as usize,
                        self.resolve_u64(value),
                    );
                    None
                }
                Statement::PhiNode { .. } => todo!(),

                Statement::Jump { target } => return BlockResult::NextBlock(*target),
                Statement::Branch {
                    condition,
                    true_target,
                    false_target,
                } => {
                    let condition = self.resolve_u64(condition);

                    return BlockResult::NextBlock(if condition != 0 {
                        *true_target
                    } else {
                        *false_target
                    });
                }

                Statement::Return { value } => {
                    return BlockResult::ReturnValue(value.map(|value| self.resolve(value)));
                }

                Statement::Panic(v) => {
                    let v = self.resolve(v);
                    panic!("panic! {v:?}")
                }

                Statement::Assert { condition } => {
                    let condition = self.resolve_u64(condition);
                    if condition == 0 {
                        panic!(
                            "{}: block {block_ref:?}: {statement_ref:?} assert failed: {condition:?} != 0",
                            self.function_name
                        );
                    }

                    None
                }
            };

            log::trace!(
                "{}: block {block_ref:?}: {statement_ref:?} = {value:?}",
                self.function_name
            );

            if let Some(value) = value {
                self.statement_values.insert(*statement_ref, value);
            }
        }

        unreachable!("block must end in a panic, jump, return, or branch")
    }

    fn write_register(&mut self, value: Value, offset: usize) {
        let (value, width) = match value {
            Value::UnsignedInteger { value, width } => (value, width),
            Value::SignedInteger { value, width } => (value as u64 & mask(width), width),
            Value::Vector(values) => {
                // assumes all vector values have the same width
                let Some(Value::UnsignedInteger { width, .. } | Value::SignedInteger { width, .. }) =
                    values.first()
                else {
                    todo!()
                };
                let width = usize::try_from(*width).unwrap();

                values
                    .into_iter()
                    .enumerate()
                    .map(|(i, value)| (i * width, value))
                    .for_each(|(offset, value)| {
                        self.write_register(value, offset);
                    });
                return;
            }
            t => todo!("{t:?}"),
        };

        match width {
            1..=8 => self
                .register_file
                .write_raw(offset, u8::try_from(value).unwrap()),
            9..=16 => self
                .register_file
                .write_raw(offset, u16::try_from(value).unwrap()),
            17..=32 => self
                .register_file
                .write_raw(offset, u32::try_from(value).unwrap()),
            33..=64 => self.register_file.write_raw(offset, value),
            65..=128 => {
                self.register_file.write_raw(offset, value);
                self.register_file.write_raw(offset + 8, 0u64); // todo: hack
            }
            w => {
                log::trace!(
                    "tried to write {value} to a {w} bit register offset {offset}, did nothing"
                );
            }
        }
    }

    fn read_register(&self, typ: &Type, offset: usize) -> Value {
        match typ {
            Type::Primitive(ptyp) => {
                let value = match ptyp.width() {
                    1..=8 => self.register_file.read_raw::<u8>(offset) as u64,
                    9..=16 => self.register_file.read_raw::<u16>(offset) as u64,
                    17..=32 => self.register_file.read_raw::<u32>(offset) as u64,
                    33..=64 => self.register_file.read_raw::<u64>(offset),
                    65..=128 => u64::try_from(self.register_file.read_raw::<u128>(offset)).unwrap(),

                    w => {
                        log::trace!(
                            "tried to read a {w} bit register offset {offset}, returning 0"
                        );
                        0
                    }
                };

                match ptyp {
                    PrimitiveType::UnsignedInteger(width) => Value::UnsignedInteger {
                        value: value & mask(*width),
                        width: *width,
                    },
                    PrimitiveType::SignedInteger(width) => Value::SignedInteger {
                        value: (value & mask(*width)) as i64,
                        width: *width,
                    },
                    _ => todo!("{typ}"),
                }
            }
            Type::Vector {
                element_count,
                element_type,
            } => {
                let element_width = element_type.width_bytes();

                Value::Vector(
                    (0..*element_count)
                        .into_iter()
                        .map(|i| offset + (i * usize::try_from(element_width).unwrap()))
                        .map(|element_offset| self.read_register(&element_type, element_offset))
                        .collect(),
                )
            }
            t => todo!("{t}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    UnsignedInteger { value: u64, width: u32 },
    SignedInteger { value: i64, width: u32 },
    FloatingPoint(f64),
    String(InternedString),
    Vector(Vec<Value>),
    Tuple(Vec<Value>),
}

impl Value {
    pub fn from_constant(value: &Constant) -> Self {
        match value {
            Constant::UnsignedInteger { value, width } => Value::UnsignedInteger {
                value: *value,
                width: *width,
            },
            Constant::SignedInteger { value, width } => Value::SignedInteger {
                value: *value,
                width: *width,
            },
            Constant::FloatingPoint { value, .. } => Value::FloatingPoint(*value),
            Constant::String(interned_string) => Value::String(*interned_string),

            Constant::Tuple(vec) => Value::Tuple(vec.iter().map(Value::from_constant).collect()),
            Constant::Vector(vec) => Value::Vector(vec.iter().map(Value::from_constant).collect()),
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Value) -> Option<Ordering> {
        match (self, other) {
            (
                Value::UnsignedInteger {
                    value: left,
                    width: _left_length,
                },
                Value::UnsignedInteger {
                    value: right,
                    width: _right_length,
                },
            ) => {
                //assert_eq!(left_length, right_length);
                left.partial_cmp(right)
            }
            (
                Value::SignedInteger {
                    value: left,
                    width: _left_length,
                },
                Value::SignedInteger {
                    value: right,
                    width: _right_length,
                },
            ) => {
                //assert_eq!(left_length, right_length);
                left.partial_cmp(right)
            }
            (l, r) => todo!("{l:?} {r:?}"),
        }
    }
}

impl Sub for Value {
    type Output = Value;

    fn sub(self, rhs: Value) -> Self::Output {
        match (self, rhs) {
            (
                Value::SignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::SignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::SignedInteger {
                value: left - right,
                width: max(left_length, right_length),
            },
            (
                Value::UnsignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::UnsignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::UnsignedInteger {
                value: left - right,
                width: max(left_length, right_length),
            },
            (
                Value::SignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::UnsignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::SignedInteger {
                value: left - i64::try_from(right).unwrap(),
                width: max(left_length, right_length),
            },
            (left, right) => todo!("{left:?} {right:?}"),
        }
    }
}

impl Add for Value {
    type Output = Value;

    fn add(self, rhs: Value) -> Self::Output {
        match (self, rhs) {
            (
                Value::SignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::SignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::SignedInteger {
                value: left + right,
                width: max(left_length, right_length),
            },
            (
                Value::UnsignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::SignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::SignedInteger {
                value: i64::try_from(left).unwrap() + right,
                width: max(left_length, right_length),
            },
            (
                Value::SignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::UnsignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::SignedInteger {
                value: left + i64::try_from(right).unwrap(),
                width: max(left_length, right_length),
            },
            (left, right) => todo!("{left:?} {right:?}"),
        }
    }
}

impl Mul for Value {
    type Output = Value;

    fn mul(self, rhs: Value) -> Self::Output {
        match (self, rhs) {
            (
                Value::SignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::SignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::SignedInteger {
                value: left * right,
                width: max(left_length, right_length),
            },
            (
                Value::SignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::UnsignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::SignedInteger {
                value: left * i64::try_from(right).unwrap(),
                width: max(left_length, right_length),
            },
            (
                Value::UnsignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::UnsignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::UnsignedInteger {
                value: left * right,
                width: max(left_length, right_length),
            },
            (left, right) => todo!("{left:?} {right:?}"),
        }
    }
}

impl BitOr for Value {
    type Output = Value;

    fn bitor(self, rhs: Value) -> Self::Output {
        match (self, rhs) {
            (
                Value::UnsignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::UnsignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::UnsignedInteger {
                value: left | right,
                width: max(left_length, right_length),
            },
            (left, right) => todo!("{left:?} {right:?}"),
        }
    }
}

impl BitAnd for Value {
    type Output = Value;

    fn bitand(self, rhs: Value) -> Self::Output {
        match (self, rhs) {
            (
                Value::UnsignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::UnsignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::UnsignedInteger {
                value: left & right,
                width: max(left_length, right_length),
            },
            (left, right) => todo!("{left:?} {right:?}"),
        }
    }
}
impl Div for Value {
    type Output = Value;

    fn div(self, rhs: Value) -> Self::Output {
        match (self, rhs) {
            (
                Value::SignedInteger {
                    value: left,
                    width: left_length,
                },
                Value::SignedInteger {
                    value: right,
                    width: right_length,
                },
            ) => Value::SignedInteger {
                value: left / right,
                width: max(left_length, right_length),
            },
            (left, right) => todo!("{left:?} {right:?}"),
        }
    }
}

enum BlockResult {
    NextBlock(Ref<Block>),
    ReturnValue(Option<Value>),
}

fn sign_extend(value: i64, source_width: u32, dest_width: u32) -> i64 {
    let shift_amount = i64::BITS - source_width;

    let signed_extended = (value << shift_amount) >> shift_amount;

    ((signed_extended as u64) & mask(dest_width)) as i64
}
