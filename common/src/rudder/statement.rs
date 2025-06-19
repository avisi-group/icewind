use {
    crate::{
        arena::{Arena, Ref},
        hashmap::HashMap,
        intern::InternedString,
        rudder::{
            block::Block,
            constant::Constant,
            function::Symbol,
            types::{PrimitiveType, Type, maybe_type_to_string},
        },
    },
    alloc::{
        format,
        string::{String, ToString},
        vec::Vec,
    },
    core::{
        cmp::Ordering,
        fmt::{self, Debug, Display},
    },
    itertools::Itertools,
};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum UnaryOperationKind {
    Not,
    Negate,
    Complement,
    Power2,
    Absolute,
    Ceil,
    Floor,
    SquareRoot,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BinaryOperationKind {
    Add,
    Sub,
    Multiply,
    Divide,
    Modulo,
    And,
    Or,
    Xor,
    PowI,
    CompareEqual,
    CompareNotEqual,
    CompareLessThan,
    CompareLessThanOrEqual,
    CompareGreaterThan,
    CompareGreaterThanOrEqual,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TernaryOperationKind {
    AddWithCarry,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CastOperationKind {
    ZeroExtend,
    SignExtend,
    Truncate,
    Reinterpret,
    Convert,
    Broadcast,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ShiftOperationKind {
    LogicalShiftLeft,
    LogicalShiftRight,
    ArithmeticShiftRight,
    RotateRight,
    RotateLeft,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Statement {
    Constant(Constant),

    ReadVariable {
        symbol: Symbol,
    },

    WriteVariable {
        symbol: Symbol,
        value: Ref<Statement>,
    },

    ReadRegister {
        typ: Type,
        /// offset into register state
        ///
        /// During building, this should just be the `next_register_offset`
        /// value, not accessing any elements or fields
        offset: Ref<Statement>,
    },

    WriteRegister {
        /// offset into register state
        ///
        /// During building, this should just be the `next_register_offset`
        /// value, not accessing any elements or fields
        offset: Ref<Statement>,
        value: Ref<Statement>,
    },

    ReadMemory {
        address: Ref<Statement>,
        // bytes
        size: Ref<Statement>,
    },
    WriteMemory {
        address: Ref<Statement>,
        value: Ref<Statement>,
    },

    ReadPc,
    WritePc {
        value: Ref<Statement>,
    },

    GetFlags {
        operation: Ref<Statement>,
    },

    UnaryOperation {
        kind: UnaryOperationKind,
        value: Ref<Statement>,
    },
    BinaryOperation {
        kind: BinaryOperationKind,
        lhs: Ref<Statement>,
        rhs: Ref<Statement>,
    },
    TernaryOperation {
        kind: TernaryOperationKind,
        a: Ref<Statement>,
        b: Ref<Statement>,
        c: Ref<Statement>,
    },
    ShiftOperation {
        kind: ShiftOperationKind,
        value: Ref<Statement>,
        amount: Ref<Statement>,
    },
    Call {
        target: InternedString, // todo: ref<function>
        args: Vec<Ref<Statement>>,
        return_type: Option<Type>, /* todo: this is really bad. necessary to avoid needing to
                                    * pass a rudder model
                                    * into every .typ() call, and hopefully a function
                                    * return type is unlikely to change after boom, but this
                                    * should really be a
                                    * function lookup */
    },
    Cast {
        kind: CastOperationKind,
        typ: Type,
        value: Ref<Statement>,
    },
    BitsCast {
        kind: CastOperationKind,
        typ: Type,
        value: Ref<Statement>,
        width: Ref<Statement>,
    },
    Jump {
        target: Ref<Block>,
    },
    Branch {
        condition: Ref<Statement>,
        true_target: Ref<Block>,
        false_target: Ref<Block>,
    },
    PhiNode {
        members: Vec<(Ref<Block>, Ref<Statement>)>,
    },
    Return {
        value: Option<Ref<Statement>>,
    },
    Select {
        condition: Ref<Statement>,
        true_value: Ref<Statement>,
        false_value: Ref<Statement>,
    },
    BitExtract {
        value: Ref<Statement>,
        start: Ref<Statement>,
        width: Ref<Statement>,
    },
    BitInsert {
        /// Target data that `width` bits of `source` will be inserted into at
        /// position `start`
        target: Ref<Statement>,
        /// Source bits that will be inserted into target
        source: Ref<Statement>,
        /// Offset into `target` that `source` will be inserted
        start: Ref<Statement>,
        /// Width of `source` that will be inserted
        width: Ref<Statement>,
    },

    BitReplicate {
        pattern: Ref<Statement>,
        count: Ref<Statement>,
    },
    ReadElement {
        vector: Ref<Statement>,
        index: Ref<Statement>,
    },
    /// Returns the vector with the mutated element
    AssignElement {
        vector: Ref<Statement>,
        value: Ref<Statement>,
        index: Ref<Statement>,
    },

    /// Fatal error, printing value of supplied Ref<StatementInner> for
    /// debugging purposes
    Panic(Ref<Statement>),

    Assert {
        condition: Ref<Statement>,
    },

    CreateBits {
        value: Ref<Statement>,
        width: Ref<Statement>,
    },

    // creating bits and getting the value done through casting
    // gets the width when applied to bits
    SizeOf {
        value: Ref<Statement>,
    },

    /// Tests whether an instance of a union is of a given variant
    MatchesUnion {
        value: Ref<Statement>,
        variant: InternedString,
    },

    /// Extracts the contents of a variant of a union
    UnwrapUnion {
        value: Ref<Statement>,
        variant: InternedString,
    },

    CreateTuple(Vec<Ref<Statement>>),
    TupleAccess {
        index: usize,
        source: Ref<Statement>,
    },
}

impl Display for Ref<Statement> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "s{}", self.index())
    }
}

impl Ref<Statement> {
    pub fn to_string(&self, arena: &Arena<Statement>) -> String {
        format!("{self}{}", self.get(arena).to_string(arena))
    }
}

impl Statement {
    pub fn typ(&self, arena: &Arena<Statement>) -> Option<Type> {
        match self {
            Self::Constant(c) => Some(c.typ()),
            Self::ReadVariable { symbol } => Some(symbol.typ()),
            Self::WriteVariable { .. } => None,
            Self::ReadRegister { typ, .. } => Some(typ.clone()),
            Self::WriteRegister { .. } => None,
            Self::ReadMemory { .. } => Some(Type::Bits),
            Self::WriteMemory { .. } => None,
            Self::BinaryOperation {
                kind: BinaryOperationKind::CompareEqual,
                ..
            }
            | Self::BinaryOperation {
                kind: BinaryOperationKind::CompareNotEqual,
                ..
            }
            | Self::BinaryOperation {
                kind: BinaryOperationKind::CompareGreaterThanOrEqual,
                ..
            }
            | Self::BinaryOperation {
                kind: BinaryOperationKind::CompareGreaterThan,
                ..
            }
            | Self::BinaryOperation {
                kind: BinaryOperationKind::CompareLessThanOrEqual,
                ..
            }
            | Self::BinaryOperation {
                kind: BinaryOperationKind::CompareLessThan,
                ..
            } => Some(Type::u1()),
            Self::BinaryOperation { lhs, .. } => lhs.get(arena).typ(arena),
            Self::TernaryOperation { a, .. } => a.get(arena).typ(arena),
            Self::UnaryOperation {
                kind: UnaryOperationKind::Ceil | UnaryOperationKind::Floor,
                ..
            } => Some(Type::s64()),
            Self::UnaryOperation { value, .. } => value.get(arena).typ(arena),
            Self::ShiftOperation { value, .. } => value.get(arena).typ(arena),

            Self::Call { return_type, .. } => return_type.clone(),

            Self::Cast { typ, .. } | Self::BitsCast { typ, .. } => Some(typ.clone()),
            Self::Jump { .. } => None,
            Self::Branch { .. } => None,
            Self::PhiNode { members } => members
                .first()
                .map(|(_, stmt)| stmt.get(arena).typ(arena))
                .flatten(),

            Self::Return { .. } => None,
            Self::Select { true_value, .. } => true_value.get(arena).typ(arena),
            Self::Panic(_) => None,

            Self::ReadPc => Some(Type::u64()),
            Self::WritePc { .. } => None,
            // todo: this is a simplification, be more precise about lengths?
            Self::BitExtract {
                value,
                width: length,
                ..
            } => {
                if let Self::Constant(length) = length.get(arena) {
                    Some(match length {
                        Constant::UnsignedInteger { value: l, .. } => Type::new_primitive(
                            PrimitiveType::UnsignedInteger(u16::try_from(*l).unwrap()),
                        ),
                        Constant::SignedInteger { value: l, .. } => Type::new_primitive(
                            PrimitiveType::UnsignedInteger(u16::try_from(*l).unwrap()),
                        ),
                        _ => panic!("non unsigned integer length: {length:#?}"),
                    })
                } else {
                    value.get(arena).typ(arena) // potentially should be Bits,
                    // but this type will always be
                    // wide enough (for example,
                    // extracted 32 bits from a u64,
                    // not the end of the world to
                    // store those 32 bits in a u64,
                    // but ideally a u32)
                }
            }
            Self::BitInsert {
                target: original_value,
                ..
            } => original_value.get(arena).typ(arena),
            Self::BitReplicate { .. } => Some(Type::Bits), /* todo: calculate more accurate type
                                                             * if */
            // possible?
            Self::ReadElement { vector, .. } => {
                let Some(Type::Vector { element_type, .. }) = &vector.get(arena).typ(arena) else {
                    panic!("cannot read field of non-composite type")
                };

                Some((**element_type).clone())
            }
            Self::AssignElement { vector, .. } => {
                // get type of the vector and return it
                vector.get(arena).typ(arena)
            }

            Self::SizeOf { .. } => Some(Type::u16()),
            Self::Assert { .. } => None,
            Self::CreateBits { .. } => Some(Type::Bits),
            Self::MatchesUnion { .. } => Some(Type::u1()),
            Self::UnwrapUnion { .. } => {
                // let Type::Enum(variants) = &*value.get(arena).typ(arena) else {
                //     panic!("cannot unwrap non sum type");
                // };

                // variants
                //     .iter()
                //     .find(|(name, _)| *name == variant)
                //     .unwrap()
                //     .1
                //     .clone()
                todo!()
            }

            Self::TupleAccess { index, source } => {
                let Some(Type::Tuple(ts)) = &source.get(arena).typ(arena) else {
                    panic!();
                };

                Some(ts[*index].clone())
            }

            Self::GetFlags { .. } => Some(Type::new_primitive(PrimitiveType::UnsignedInteger(4))),
            Self::CreateTuple(values) => Some(Type::Tuple(
                values
                    .iter()
                    .map(|v| {
                        v.get(arena)
                            .typ(arena)
                            .expect("why are you making a tuple that contains a void")
                    })
                    .collect(),
            )),
        }
    }

    pub fn replace_kind(&mut self, kind: Statement) {
        *self = kind;
    }

    pub fn replace_use(&mut self, use_of: Ref<Statement>, with: Ref<Statement>) {
        match self.clone() {
            Self::Return { value } => {
                let value = value.map(|value| {
                    if value == use_of {
                        with.clone()
                    } else {
                        value.clone()
                    }
                });
                *self = Self::Return { value };
            }
            Self::Branch {
                true_target,
                false_target,
                condition,
            } => {
                let condition = if condition == use_of {
                    with.clone()
                } else {
                    condition.clone()
                };

                *self = Self::Branch {
                    condition,
                    true_target,
                    false_target,
                };
            }
            Self::WriteVariable { symbol, value } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                *self = Self::WriteVariable { symbol, value };
            }
            Self::BinaryOperation { kind, lhs, rhs } => {
                if lhs == use_of {
                    *self = Self::BinaryOperation {
                        kind,
                        lhs: with.clone(),
                        rhs,
                    };
                } else if rhs == use_of {
                    *self = Self::BinaryOperation {
                        kind,
                        lhs,
                        rhs: with.clone(),
                    };
                } else {
                    panic!("should not get here");
                }
            }
            Self::TernaryOperation { kind, a, b, c } => {
                let a = if a == use_of { with.clone() } else { a.clone() };
                let b = if b == use_of { with.clone() } else { b.clone() };
                let c = if c == use_of { with.clone() } else { c.clone() };

                *self = Self::TernaryOperation { kind, a, b, c };
            }
            Self::UnaryOperation { kind, value } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                *self = Self::UnaryOperation { kind, value };
            }

            Self::Cast { kind, typ, value } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                *self = Self::Cast { kind, typ, value };
            }
            Self::BitsCast {
                kind,
                typ,
                value,
                width: length,
            } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                let length = if length == use_of {
                    with.clone()
                } else {
                    length.clone()
                };

                *self = Self::BitsCast {
                    kind,
                    typ,
                    value,
                    width: length,
                };
            }
            Self::Call {
                target,
                args,
                return_type,
            } => {
                let args = args
                    .iter()
                    .map(|arg| {
                        if *arg == use_of {
                            with.clone()
                        } else {
                            arg.clone()
                        }
                    })
                    .collect();

                *self = Self::Call {
                    target,
                    args,
                    return_type,
                };
            }
            Self::BitExtract {
                value,
                start,
                width: length,
            } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                let start = if start == use_of {
                    with.clone()
                } else {
                    start.clone()
                };

                let length = if length == use_of {
                    with.clone()
                } else {
                    length.clone()
                };

                *self = Self::BitExtract {
                    value,
                    start,
                    width: length,
                };
            }
            Self::BitReplicate { pattern, count } => {
                let pattern = if pattern == use_of {
                    with.clone()
                } else {
                    pattern.clone()
                };

                let count = if count == use_of {
                    with.clone()
                } else {
                    count.clone()
                };

                *self = Self::BitReplicate { pattern, count };
            }

            Self::Assert { condition } => {
                let condition = if condition == use_of {
                    with.clone()
                } else {
                    condition.clone()
                };

                *self = Self::Assert { condition };
            }
            Self::ShiftOperation {
                kind,
                value,
                amount,
            } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                let amount = if amount == use_of {
                    with.clone()
                } else {
                    amount.clone()
                };

                *self = Self::ShiftOperation {
                    kind,
                    value,
                    amount,
                };
            }
            Self::WriteRegister { offset, value } => {
                let offset = if offset == use_of {
                    with.clone()
                } else {
                    offset.clone()
                };

                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                *self = Self::WriteRegister { offset, value };
            }
            Self::WriteMemory {
                address: offset,
                value,
            } => {
                let offset = if offset == use_of {
                    with.clone()
                } else {
                    offset.clone()
                };

                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                *self = Self::WriteMemory {
                    address: offset,
                    value,
                }
            }
            Self::ReadMemory {
                address: offset,
                size,
            } => {
                let offset = if offset == use_of {
                    with.clone()
                } else {
                    offset.clone()
                };

                let size = if size == use_of {
                    with.clone()
                } else {
                    size.clone()
                };

                *self = Self::ReadMemory {
                    address: offset,
                    size,
                }
            }

            Self::ReadElement { vector, index } => {
                let vector = if vector == use_of {
                    with.clone()
                } else {
                    vector.clone()
                };

                let index = if index == use_of {
                    with.clone()
                } else {
                    index.clone()
                };

                *self = Self::ReadElement { vector, index };
            }

            Self::BitInsert {
                target: original_value,
                source: insert_value,
                start,
                width: length,
            } => {
                let stmts = [original_value, insert_value, start, length]
                    .into_iter()
                    .map(|s| if s == use_of { with.clone() } else { s })
                    .collect::<Vec<_>>();

                *self = Self::BitInsert {
                    target: stmts[0].clone(),
                    source: stmts[1].clone(),
                    start: stmts[2].clone(),
                    width: stmts[3].clone(),
                }
            }

            Self::SizeOf { value } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };
                *self = Self::SizeOf { value };
            }

            Self::Select {
                condition,
                true_value,
                false_value,
            } => {
                let condition = if condition == use_of {
                    with.clone()
                } else {
                    condition.clone()
                };

                let true_value = if true_value == use_of {
                    with.clone()
                } else {
                    true_value.clone()
                };

                let false_value = if false_value == use_of {
                    with.clone()
                } else {
                    false_value.clone()
                };

                *self = Self::Select {
                    condition,
                    true_value,
                    false_value,
                };
            }

            Self::AssignElement {
                vector,
                value,
                index,
            } => {
                let vector = if vector == use_of {
                    with.clone()
                } else {
                    vector.clone()
                };

                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                let index = if index == use_of {
                    with.clone()
                } else {
                    index.clone()
                };

                *self = Self::AssignElement {
                    vector,
                    value,
                    index,
                };
            }
            Self::WritePc { value } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                *self = Self::WritePc { value };
            }
            Self::Panic(value) => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                *self = Self::Panic(value)
            }

            Self::CreateBits {
                value,
                width: length,
            } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                let length = if length == use_of {
                    with.clone()
                } else {
                    length.clone()
                };

                *self = Self::CreateBits {
                    value,
                    width: length,
                };
            }
            Self::MatchesUnion { value, variant } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                *self = Self::MatchesUnion { value, variant };
            }
            Self::UnwrapUnion { value, variant } => {
                let value = if value == use_of {
                    with.clone()
                } else {
                    value.clone()
                };

                *self = Self::UnwrapUnion { value, variant };
            }

            Self::Constant { .. } => todo!(),
            Self::ReadVariable { .. } => todo!(),
            Self::ReadRegister { .. } => todo!(),
            Self::ReadPc => todo!(),
            Self::Jump { .. } => todo!(),
            Self::PhiNode { .. } => todo!(),
            Self::TupleAccess { index, source } => {
                let source = if source == use_of {
                    with.clone()
                } else {
                    source.clone()
                };

                *self = Self::TupleAccess { index, source };
            }

            Self::CreateTuple(values) => {
                *self = Self::CreateTuple(
                    values
                        .iter()
                        .map(|v| {
                            if *v == use_of {
                                with.clone()
                            } else {
                                v.clone()
                            }
                        })
                        .collect(),
                )
            }
            Self::GetFlags { operation } => {
                let operation = if operation == use_of {
                    with.clone()
                } else {
                    operation.clone()
                };

                *self = Self::GetFlags { operation };
            }
        }
    }

    pub fn to_string(&self, arena: &Arena<Statement>) -> String {
        let operation = match &self {
            Self::Constant(c) => format!("const #{c}"),
            Self::ReadVariable { symbol } => {
                format!("read-var {}:{}", symbol.name(), symbol.typ())
            }
            Self::WriteVariable { symbol, value } => {
                format!(
                    "write-var {}:{} <= {}:{}",
                    symbol.name(),
                    symbol.typ(),
                    value,
                    maybe_type_to_string(value.get(arena).typ(arena))
                )
            }
            Self::ReadRegister { typ, offset } => {
                format!("read-reg {}:{}", offset, typ)
            }
            Self::WriteRegister { offset, value } => {
                format!("write-reg {} <= {}", offset, value)
            }
            Self::ReadMemory {
                address: offset,
                size,
            } => {
                format!("read-mem {}:{}", offset, size)
            }
            Self::WriteMemory {
                address: offset,
                value,
            } => {
                format!("write-mem {} <= {}", offset, value)
            }
            Self::BinaryOperation { kind, lhs, rhs } => {
                let op = match kind {
                    BinaryOperationKind::Add => "add",
                    BinaryOperationKind::Sub => "sub",
                    BinaryOperationKind::Multiply => "mul",
                    BinaryOperationKind::Divide => "div",
                    BinaryOperationKind::Modulo => "mod",
                    BinaryOperationKind::CompareEqual => "cmp-eq",
                    BinaryOperationKind::CompareNotEqual => "cmp-ne",
                    BinaryOperationKind::CompareLessThan => "cmp-lt",
                    BinaryOperationKind::CompareLessThanOrEqual => "cmp-le",
                    BinaryOperationKind::CompareGreaterThan => "cmp-gt",
                    BinaryOperationKind::CompareGreaterThanOrEqual => "cmp-ge",
                    BinaryOperationKind::And => "and",
                    BinaryOperationKind::Or => "or",
                    BinaryOperationKind::Xor => "xor",
                    BinaryOperationKind::PowI => "powi",
                };

                format!("{} {} {}", op, lhs, rhs)
            }
            Self::TernaryOperation { kind, a, b, c } => {
                let op = match kind {
                    TernaryOperationKind::AddWithCarry => "add-with-carry",
                };

                format!("{} {} {} {}", op, a, b, c)
            }
            Self::UnaryOperation { kind, value } => {
                let op = match kind {
                    UnaryOperationKind::Complement => "cmpl",
                    UnaryOperationKind::Not => "not",
                    UnaryOperationKind::Negate => "neg",
                    UnaryOperationKind::Power2 => "pow2",
                    UnaryOperationKind::Absolute => "abs",
                    UnaryOperationKind::Ceil => "ceil",
                    UnaryOperationKind::Floor => "floor",
                    UnaryOperationKind::SquareRoot => "sqrt",
                };

                format!("{} {}", op, value)
            }

            Self::ShiftOperation {
                kind,
                value,
                amount,
            } => {
                let op = match kind {
                    ShiftOperationKind::LogicalShiftLeft => "lsl",
                    ShiftOperationKind::LogicalShiftRight => "lsr",
                    ShiftOperationKind::ArithmeticShiftRight => "asr",
                    ShiftOperationKind::RotateRight => "ror",
                    ShiftOperationKind::RotateLeft => "rol",
                };

                format!("{} {} {}", op, value, amount)
            }
            Self::Call { target, args, .. } => {
                format!("call {}({})", target, args.iter().map(|s| s).join(", "))
            }
            Self::Cast { kind, typ, value } => {
                let op = match kind {
                    CastOperationKind::ZeroExtend => "zx",
                    CastOperationKind::SignExtend => "sx",
                    CastOperationKind::Truncate => "trunc",
                    CastOperationKind::Reinterpret => "reint",
                    CastOperationKind::Convert => "cvt",
                    CastOperationKind::Broadcast => "bcast",
                };

                format!("cast {} {} -> {}", op, value, typ)
            }
            Self::BitsCast {
                kind,
                typ,
                value,
                width: length,
            } => {
                let op = match kind {
                    CastOperationKind::ZeroExtend => "zx",
                    CastOperationKind::SignExtend => "sx",
                    CastOperationKind::Truncate => "trunc",
                    CastOperationKind::Reinterpret => "reint",
                    CastOperationKind::Convert => "cvt",
                    CastOperationKind::Broadcast => "bcast",
                };

                format!("bits-cast {} {} -> {} length {}", op, value, typ, length)
            }
            Self::Jump { target } => format!("jump block {:#x}", target.index()), /* todo: type */
            // for target
            // that formats
            // to block {:
            // #x}, or maybe
            // fancy display
            // T {:#x} for
            // Ref<T>
            Self::Branch {
                condition,
                true_target,
                false_target,
            } => {
                format!(
                    "branch {} ? block {:#x} : block {:#x}",
                    condition,
                    true_target.index(),
                    false_target.index(),
                )
            }
            Self::PhiNode { .. } => {
                // format!( "phi ")?;

                // for member in members {
                //     format!( "(BLOCK, {}) ", member.1)?;
                // }

                // Ok(())
                todo!()
            }

            Self::Return { value } => {
                format!(
                    "return {}",
                    value.as_ref().map(ToString::to_string).unwrap_or_default()
                )
            }
            Self::Select {
                condition,
                true_value,
                false_value,
            } => {
                format!("select {} {} {}", condition, true_value, false_value)
            }
            Self::Panic(statement) => {
                format!("panic {}", statement)
            }

            Self::ReadPc => format!("read-pc"),
            Self::WritePc { value } => format!("write-pc {}", value),
            Self::BitExtract {
                value,
                start,
                width: length,
            } => format!("bit-extract {} {} {}", value, start, length),
            Self::BitInsert {
                target: original_value,
                source: insert_value,
                start,
                width: length,
            } => format!(
                "bit-insert {} {} {} {}",
                original_value, insert_value, start, length
            ),
            Self::BitReplicate { pattern, count } => {
                format!("bit-replicate {} {}", pattern, count)
            }
            Self::ReadElement { vector, index } => {
                format!("read-element {}[{}]", vector, index)
            }
            Self::AssignElement {
                vector,
                value,
                index,
            } => format!("mutate-element {}[{}] <= {}", vector, index, value),

            Self::SizeOf { value } => {
                format!("size-of {}", value)
            }
            Self::Assert { condition } => {
                format!("assert {}", condition)
            }

            Self::CreateBits {
                value,
                width: length,
            } => {
                format!("create-bits {} {}", value, length)
            }
            Self::MatchesUnion { value, variant } => {
                format!("matches-union {} {variant}", value)
            }
            Self::UnwrapUnion { value, variant } => {
                format!("unwrap-union {} {variant}", value)
            }
            Self::TupleAccess { index, source } => {
                format!("tuple-access {}.{index}", source)
            }
            Self::GetFlags { operation } => {
                format!("get-flags {operation}")
            }
            Self::CreateTuple(values) => {
                format!(
                    "create-tuple {:?}",
                    values.iter().map(|v| v.index()).collect::<Vec<_>>()
                )
            }
        };

        if let Some(typ) = self.typ(arena) {
            format!(":{} = {operation}", typ)
        } else {
            format!(" = {operation}")
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Location {
    End,
    Before(Ref<Statement>),
}

/// Creates a new statement in the block's arena, and inserts it at the supplied
/// location
pub fn build_at(
    block: Ref<Block>,
    arena: &mut Arena<Block>,
    statement: Statement,
    location: Location,
) -> Ref<Statement> {
    let r = block.get_mut(arena).arena_mut().insert(statement);
    match location {
        Location::Before(before) => block.get_mut(arena).insert_statement_before(before, r),
        Location::End => block.get_mut(arena).append_statement(r),
    }
    r
}

/// Creates a new statement in the block's arena, and pushes it to the end of
/// the block's statements
pub fn build(block: Ref<Block>, arena: &mut Arena<Block>, statement: Statement) -> Ref<Statement> {
    build_at(block, arena, statement, Location::End)
}

pub fn cast(
    block: Ref<Block>,
    arena: &mut Arena<Block>,
    source: Ref<Statement>,
    destination_type: Type,
) -> Ref<Statement> {
    cast_at(block, arena, source, destination_type, Location::End)
}

// No-op if same type
pub fn cast_at(
    block: Ref<Block>,
    arena: &mut Arena<Block>,
    source: Ref<Statement>,
    destination_type: Type,
    location: Location,
) -> Ref<Statement> {
    let s_arena = block.get(arena).arena();

    let source_type = source.get(s_arena).typ(s_arena).unwrap();

    if source_type == destination_type {
        return source;
    }

    match (&source_type, &destination_type) {
        // both primitives, do a cast
        (Type::Primitive(source_primitive), Type::Primitive(dest_primitive)) => {
            // compare widths
            match source_primitive.width().cmp(&dest_primitive.width()) {
                // source is larger than destination
                Ordering::Greater => build_at(
                    block,
                    arena,
                    Statement::Cast {
                        kind: CastOperationKind::Truncate,
                        typ: destination_type,
                        value: source,
                    },
                    location,
                ),

                // destination is larger than source
                Ordering::Less => {
                    let kind = match source_primitive {
                        PrimitiveType::UnsignedInteger(_) => CastOperationKind::ZeroExtend,
                        PrimitiveType::SignedInteger(_) => CastOperationKind::SignExtend,
                        PrimitiveType::FloatingPoint(_) => CastOperationKind::SignExtend,
                    };

                    build_at(
                        block,
                        arena,
                        Statement::Cast {
                            kind,
                            typ: destination_type,
                            value: source,
                        },
                        location,
                    )
                }

                // equal width
                Ordering::Equal => build_at(
                    block,
                    arena,
                    Statement::Cast {
                        kind: CastOperationKind::Reinterpret,
                        typ: destination_type,
                        value: source,
                    },
                    location,
                ),
            }
        }

        (
            Type::Vector {
                element_count: src_count,
                element_type: src_type,
            },
            Type::Vector {
                element_count: dst_count,
                element_type: dst_type,
            },
        ) => {
            if src_type != dst_type {
                todo!();
            }

            match (src_count, dst_count) {
                (0, 0) => panic!("no cast needed, both unknown"),
                (_, 0) => {
                    // casting fixed to unknown
                    build_at(
                        block,
                        arena,
                        Statement::Cast {
                            kind: CastOperationKind::Convert,
                            typ: destination_type,
                            value: source,
                        },
                        location,
                    )
                }
                (0, _) => {
                    // casting fixed to unknown
                    build_at(
                        block,
                        arena,
                        Statement::Cast {
                            kind: CastOperationKind::Convert,
                            typ: destination_type,
                            value: source,
                        },
                        location,
                    )
                }
                (_, _) => panic!("casting from fixed to fixed"),
            }
        }
        (Type::Primitive(PrimitiveType::UnsignedInteger(_)), Type::Bits) => source,
        (Type::Primitive(PrimitiveType::SignedInteger(width)), Type::Bits) => {
            if *width > 64 {
                log::warn!(
                    "source type in cast {} -> {} exceeds 64 bits",
                    source_type,
                    destination_type
                );
            }

            panic!(
                "{} ({}) to {}",
                source.get(s_arena).to_string(s_arena),
                &source_type,
                &destination_type
            );

            // // if the panic ever occurs for a good reason, use this:
            // build_at(
            //     block,
            //     arena,
            //     Statement::Cast {
            //         kind: CastOperationKind::Convert,
            //         typ: Type::Primitive(PrimitiveType {
            //             tc: PrimitiveTypeClass::UnsignedInteger,
            //             element_width_in_bits: *element_width_in_bits,
            //         }),
            //         value: source,
            //     },
            //     location,
            // )
        }

        (Type::Bits, Type::Primitive(_)) => build_at(
            block,
            arena,
            Statement::Cast {
                kind: CastOperationKind::Reinterpret,
                typ: destination_type,
                value: source,
            },
            location,
        ),

        (Type::String, Type::Tuple(t)) => {
            if *t
                == [
                    Type::Primitive(PrimitiveType::SignedInteger(64)),
                    Type::Primitive(PrimitiveType::SignedInteger(64)),
                ]
            {
                let Statement::Constant(value) = source.get(s_arena) else {
                    panic!()
                };

                let Constant::String(s) = value else { panic!() };

                // todo: fix this silliness
                let (num, den) = match s.as_ref() {
                    "0.0" => (0, 1),
                    "0.5" => (1, 2),
                    "1.0" => (1, 1),
                    "2.0" => (2, 1),
                    "3.0" => (3, 1),
                    _ => todo!("{s:?}"),
                };

                let num = build_at(
                    block,
                    arena,
                    Statement::Constant(Constant::new_signed(num, 64)),
                    location,
                );

                let den = build_at(
                    block,
                    arena,
                    Statement::Constant(Constant::new_signed(den, 64)),
                    location,
                );

                build_at(
                    block,
                    arena,
                    Statement::CreateTuple(alloc::vec![num, den]),
                    location,
                )
            } else {
                panic!();
            }
        }

        (src, dst) => {
            log::error!("current block: {:?}", block.get(arena));
            panic!(
                "cannot cast {:?} from {src:?} to {dst:?}",
                source.get(s_arena)
            );
        }
    }
}

pub fn import_statement(
    source_block: Ref<Block>,
    target_block: Ref<Block>,
    block_arena: &mut Arena<Block>,
    target_statement: Ref<Statement>,
    mapping: &HashMap<Ref<Statement>, Ref<Statement>>,
) -> Ref<Statement> {
    let mapped_kind = match target_statement
        .get(target_block.get(&block_arena).arena())
        .clone()
    {
        Statement::BinaryOperation { kind, lhs, rhs } => Statement::BinaryOperation {
            kind,
            lhs: mapping.get(&lhs).unwrap().clone(),
            rhs: mapping.get(&rhs).unwrap().clone(),
        },
        Statement::TernaryOperation { kind, a, b, c } => Statement::TernaryOperation {
            kind,
            a: mapping.get(&a).unwrap().clone(),
            b: mapping.get(&b).unwrap().clone(),
            c: mapping.get(&c).unwrap().clone(),
        },
        Statement::Constant(c) => Statement::Constant(c),
        Statement::ReadVariable { symbol } => Statement::ReadVariable { symbol },
        Statement::WriteVariable { symbol, value } => Statement::WriteVariable {
            symbol,
            value: mapping.get(&value).unwrap().clone(),
        },
        Statement::ReadRegister { typ, offset } => Statement::ReadRegister {
            typ,
            offset: mapping.get(&offset).unwrap().clone(),
        },
        Statement::WriteRegister { offset, value } => Statement::WriteRegister {
            offset: mapping.get(&offset).unwrap().clone(),
            value: mapping.get(&value).unwrap().clone(),
        },
        Statement::ReadMemory {
            address: offset,
            size,
        } => Statement::ReadMemory {
            address: mapping.get(&offset).unwrap().clone(),
            size: mapping.get(&size).unwrap().clone(),
        },
        Statement::WriteMemory {
            address: offset,
            value,
        } => Statement::WriteMemory {
            address: mapping.get(&offset).unwrap().clone(),
            value: mapping.get(&value).unwrap().clone(),
        },
        Statement::ReadPc => Statement::ReadPc,
        Statement::WritePc { value } => Statement::WritePc {
            value: mapping.get(&value).unwrap().clone(),
        },
        Statement::UnaryOperation { kind, value } => Statement::UnaryOperation {
            kind,
            value: mapping.get(&value).unwrap().clone(),
        },
        Statement::ShiftOperation {
            kind,
            value,
            amount,
        } => Statement::ShiftOperation {
            kind,
            value: mapping.get(&value).unwrap().clone(),
            amount: mapping.get(&amount).unwrap().clone(),
        },
        Statement::Call {
            target,
            args,
            return_type,
        } => {
            let args = args
                .iter()
                .map(|stmt| mapping.get(stmt).unwrap().clone())
                .collect();

            Statement::Call {
                target,
                args,
                return_type,
            }
        }
        Statement::Cast { kind, typ, value } => Statement::Cast {
            kind,
            typ: typ.clone(),
            value: mapping
                .get(&value)
                .unwrap_or_else(|| {
                    panic!(
                        "{mapping:?}, {:?}",
                        target_statement
                            .get(target_block.get(&block_arena).arena())
                            .clone()
                    )
                })
                .clone(),
        },
        Statement::BitsCast {
            kind,
            typ,
            value,
            width: length,
        } => Statement::BitsCast {
            kind,
            typ: typ.clone(),
            value: mapping.get(&value).unwrap().clone(),
            width: mapping.get(&length).unwrap().clone(),
        },
        Statement::Jump { target } => Statement::Jump { target },
        Statement::Branch {
            condition,
            true_target,
            false_target,
        } => Statement::Branch {
            condition: mapping.get(&condition).unwrap().clone(),
            true_target,
            false_target,
        },
        Statement::PhiNode { .. } => todo!(),
        Statement::Return { value } => Statement::Return {
            value: value.map(|value| mapping.get(&value).unwrap().clone()),
        },
        Statement::Select {
            condition,
            true_value,
            false_value,
        } => Statement::Select {
            condition: mapping.get(&condition).unwrap().clone(),
            true_value: mapping.get(&true_value).unwrap().clone(),
            false_value: mapping.get(&false_value).unwrap().clone(),
        },
        Statement::BitExtract {
            value,
            start,
            width: length,
        } => Statement::BitExtract {
            value: mapping.get(&value).unwrap().clone(),
            start: mapping.get(&start).unwrap().clone(),
            width: mapping.get(&length).unwrap().clone(),
        },
        Statement::BitInsert {
            target,
            source,
            start,
            width: length,
        } => Statement::BitInsert {
            target: mapping.get(&target).unwrap().clone(),
            source: mapping.get(&source).unwrap().clone(),
            start: mapping.get(&start).unwrap().clone(),
            width: mapping.get(&length).unwrap().clone(),
        },
        Statement::BitReplicate { pattern, count } => Statement::BitReplicate {
            pattern: mapping.get(&pattern).unwrap().clone(),
            count: mapping.get(&count).unwrap().clone(),
        },
        Statement::ReadElement { vector, index } => Statement::ReadElement {
            vector: mapping.get(&vector).unwrap().clone(),
            index: mapping.get(&index).unwrap().clone(),
        },
        Statement::AssignElement {
            vector,
            value,
            index,
        } => Statement::AssignElement {
            vector: mapping.get(&vector).unwrap().clone(),
            value: mapping.get(&value).unwrap().clone(),
            index: mapping.get(&index).unwrap().clone(),
        },
        Statement::Panic(stmt) => Statement::Panic(mapping.get(&stmt).unwrap().clone()),

        Statement::Assert { condition } => Statement::Assert {
            condition: mapping.get(&condition).unwrap().clone(),
        },

        Statement::CreateBits {
            value,
            width: length,
        } => Statement::CreateBits {
            value: mapping.get(&value).unwrap().clone(),
            width: mapping.get(&length).unwrap().clone(),
        },
        Statement::SizeOf { value } => Statement::SizeOf {
            value: mapping.get(&value).unwrap().clone(),
        },
        Statement::MatchesUnion { value, variant } => Statement::MatchesUnion {
            value: mapping.get(&value).unwrap().clone(),
            variant,
        },
        Statement::UnwrapUnion { value, variant } => Statement::UnwrapUnion {
            value: mapping.get(&value).unwrap().clone(),
            variant,
        },

        Statement::TupleAccess { index, source } => Statement::TupleAccess {
            source: mapping.get(&source).unwrap().clone(),
            index,
        },
        Statement::GetFlags { operation } => Statement::GetFlags {
            operation: mapping.get(&operation).unwrap().clone(),
        },
        Statement::CreateTuple(values) => Statement::CreateTuple(
            values
                .iter()
                .map(|v| mapping.get(v).unwrap())
                .cloned()
                .collect(),
        ),
    };

    build(source_block, block_arena, mapped_kind)
}
