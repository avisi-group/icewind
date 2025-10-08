use {
    crate::rudder::opt::OptimizationContext,
    common::{
        arena::{Arena, Ref},
        mask::mask,
        rudder::{
            block::Block,
            constant::Constant,
            function::Function,
            statement::{BinaryOperationKind, CastOperationKind, Statement, UnaryOperationKind},
            types::{PrimitiveType, Type},
        },
    },
};

pub fn run(_ctx: &OptimizationContext, f: &mut Function) -> bool {
    let mut changed = false;

    //trace!("constant folding {}", f.name());
    for block in f.block_iter().collect::<Vec<_>>() {
        changed |= run_on_block(block, f.arena_mut());
    }

    changed
}

fn run_on_block(b: Ref<Block>, arena: &mut Arena<Block>) -> bool {
    let mut changed = false;

    for stmt in b
        .get(arena)
        .statements()
        .iter()
        .copied()
        .collect::<Vec<_>>()
    {
        changed |= run_on_stmt(stmt, b.get_mut(arena).arena_mut());
    }

    changed
}

fn run_on_stmt(stmt: Ref<Statement>, arena: &mut Arena<Statement>) -> bool {
    if matches!(stmt.get(arena), Statement::Constant { .. }) {
        return false;
    }

    match stmt.get(arena).clone() {
        Statement::UnaryOperation {
            kind: unary_op_kind,
            value,
        } => match value.get(arena).clone() {
            Statement::Constant(constant) => match unary_op_kind {
                UnaryOperationKind::Not => {
                    let constant = Statement::Constant(!constant);

                    stmt.get_mut(arena).replace(constant);

                    true
                }
                _ => false,
            },
            _ => false,
        },
        Statement::BinaryOperation { kind, lhs, rhs } => {
            match (lhs.get(arena).clone(), rhs.get(arena).clone()) {
                (Statement::Constant(lhs), Statement::Constant(rhs)) => {
                    let cv = match kind {
                        BinaryOperationKind::Add => lhs + rhs,
                        BinaryOperationKind::Sub => lhs - rhs,
                        BinaryOperationKind::Multiply => lhs * rhs,
                        BinaryOperationKind::Divide => lhs / rhs,
                        BinaryOperationKind::Modulo => todo!(),
                        BinaryOperationKind::And => todo!(),
                        BinaryOperationKind::Or => todo!(),
                        BinaryOperationKind::Xor => lhs ^ rhs,
                        BinaryOperationKind::PowI => lhs.powi(rhs),
                        BinaryOperationKind::CompareEqual => {
                            Constant::new_unsigned((lhs == rhs) as u64, 1)
                        }
                        BinaryOperationKind::CompareNotEqual => {
                            Constant::new_unsigned((lhs != rhs) as u64, 1)
                        }
                        BinaryOperationKind::CompareLessThan => {
                            Constant::new_unsigned((lhs < rhs) as u64, 1)
                        }
                        BinaryOperationKind::CompareLessThanOrEqual => {
                            Constant::new_unsigned((lhs <= rhs) as u64, 1)
                        }
                        BinaryOperationKind::CompareGreaterThan => {
                            Constant::new_unsigned((lhs > rhs) as u64, 1)
                        }
                        BinaryOperationKind::CompareGreaterThanOrEqual => {
                            Constant::new_unsigned((lhs >= rhs) as u64, 1)
                        }
                    };

                    stmt.get_mut(arena).replace(Statement::Constant(cv));

                    true
                }
                (lhs, Statement::Constant(rhs)) => match kind {
                    BinaryOperationKind::Multiply => match rhs {
                        Constant::UnsignedInteger {
                            value: rhs_value, ..
                        } => {
                            if rhs_value == 0 {
                                false
                            } else if rhs_value == 1 {
                                false
                            } else if rhs_value == 8 {
                                //stmt.get_mut(arena).replace_kind(Statement::ShiftOperation {
                                // kind: (), value: (), amount: () });
                                false
                            } else {
                                false
                            }
                        }
                        Constant::SignedInteger {
                            value: rhs_value, ..
                        } => {
                            if rhs_value == 1 {
                                false
                            } else {
                                false
                            }
                        }
                        _ => false,
                    },
                    BinaryOperationKind::And => match rhs {
                        Constant::UnsignedInteger { value, .. } => {
                            if value == mask(lhs.typ(arena).unwrap().width_bits())
                                || value == u64::MAX
                            {
                                stmt.get_mut(arena).replace(lhs);
                                true
                            } else {
                                false
                            }
                        }
                        _ => false,
                    },
                    _ => false,
                },
                _ => false,
            }
        }
        Statement::CreateBits { value, width } => {
            let width = width.get(arena).clone();
            let value = value.get_mut(arena);

            match (value, width) {
                (Statement::Constant(value), Statement::Constant(width)) => {
                    let target_width = match width {
                        Constant::SignedInteger { value, .. } => u32::try_from(value).unwrap(),
                        Constant::UnsignedInteger { value, .. } => u32::try_from(value).unwrap(),
                        _ => {
                            panic!("const create bits from {value:?} with width {width:?}")
                        }
                    };

                    let Constant::UnsignedInteger { value, .. } = value else {
                        panic!()
                    };

                    let constant = Constant::UnsignedInteger {
                        value: *value & mask(target_width),
                        width: target_width,
                    };

                    stmt.get_mut(arena).replace(Statement::Constant(constant));

                    true
                }
                (_, Statement::Constant(width)) => {
                    // todo: replace with cast
                    false
                }
                _ => false,
            }
        }
        Statement::SizeOf { value } => {
            let target = value.get(arena);
            match target.typ(&arena) {
                // nothing to do
                Some(Type::Bits) => false,
                Some(typ) => {
                    let constant = Statement::Constant(Constant::UnsignedInteger {
                        value: typ.width_bits().into(),
                        width: 16,
                    });

                    stmt.get_mut(arena).replace(constant);

                    true
                }
                None => panic!(
                    "cannot get size of statement with no value {}",
                    target.to_string(arena)
                ),
            }
        }
        Statement::Cast {
            kind: CastOperationKind::ZeroExtend,
            typ,
            value,
        } => {
            // watch out! if you cast a constant primitive to an arbitrary bits you lose
            // length information
            if let Type::Primitive(_) = &typ {
                if let Statement::Constant(value) = value.get(arena).clone() {
                    let value = cast_integer(value, typ.clone());
                    stmt.get_mut(arena).replace(Statement::Constant(value));
                    true
                } else {
                    false
                }
            } else {
                false
            }
        }
        Statement::Cast {
            kind: CastOperationKind::Truncate,
            typ,
            value,
        } => {
            if let Statement::Constant(value) = value.get(arena).clone() {
                // creating new constant will truncate old value
                // todo: assert old width > target width
                let truncated = match typ {
                    Type::Primitive(PrimitiveType::UnsignedInteger(width)) => {
                        let value = match value {
                            Constant::UnsignedInteger { value, .. } => value,
                            Constant::SignedInteger { value, .. } => u64::try_from(value).unwrap(),
                            _ => todo!("{value}"),
                        };
                        Constant::new_unsigned(value, width)
                    }
                    Type::Primitive(PrimitiveType::SignedInteger(width)) => {
                        let value = match value {
                            Constant::SignedInteger { value, .. } => value,
                            Constant::UnsignedInteger { value, .. } => {
                                i64::try_from(value).unwrap()
                            }
                            _ => todo!("{value}"),
                        };
                        Constant::new_signed(value, width)
                    }
                    _ => todo!("{typ}"),
                };

                stmt.get_mut(arena).replace(Statement::Constant(truncated));

                true
            } else {
                false
            }
        }
        Statement::Cast {
            kind: CastOperationKind::Reinterpret,
            typ,
            value,
        } => {
            if let Statement::Constant(value) = value.get(arena) {
                let value = cast_integer(value.clone(), typ.clone());

                stmt.get_mut(arena).replace(Statement::Constant(value));
                true
            } else {
                false
            }
        }
        _ => {
            //trace!("candidate for folding not implemented: {}", stmt);
            false
        }
    }
}

fn cast_integer(value: Constant, typ: Type) -> Constant {
    match &typ {
        Type::Primitive(primitive) => match (value, primitive) {
            (Constant::UnsignedInteger { value, .. }, PrimitiveType::SignedInteger(width)) => {
                Constant::new_signed(value as i64, *width)
            }
            (Constant::SignedInteger { value, .. }, PrimitiveType::SignedInteger(width)) => {
                Constant::new_signed(value, *width)
            }
            (Constant::SignedInteger { value, .. }, PrimitiveType::UnsignedInteger(width)) => {
                Constant::new_unsigned(u64::try_from(value).unwrap(), *width)
            }
            (Constant::UnsignedInteger { value, .. }, PrimitiveType::UnsignedInteger(width)) => {
                Constant::new_unsigned(value, *width)
            }
            (Constant::SignedInteger { value, .. }, PrimitiveType::FloatingPoint(width)) => {
                Constant::new_float(f64::from(i32::try_from(value).unwrap()), *width)
            }
            (typ, cv) => {
                panic!("incompatible type {typ:?} and constant value {cv:?}")
            }
        },
        _ => panic!("failed to cast {value:x?} to type {typ:?}"),
    }
}
