//! Borealis Object Of Machine, Internal intermediate representation used to
//! convert JIB AST to GenC AST

#![allow(missing_docs)]

use {
    crate::boom::{
        control_flow::ControlFlowBlock,
        convert::BoomEmitter,
        visitor::{Visitor, Walkable},
    },
    common::{hashmap::HashMap, intern::InternedString},
    kinded::Kinded,
    num_bigint::BigInt,
    sailrs::{jib_ast, shared::Shared},
    std::{fmt::Debug, ops::Add},
};

pub mod control_flow;
pub mod convert;
pub mod passes;
pub mod pretty_print;
pub mod visitor;

/// BOOM AST
#[derive(Debug, Clone, Default)]
pub struct Ast {
    /// Register types by identifier
    pub registers: HashMap<InternedString, Shared<Type>>,
    /// Function definitions by identifier
    pub functions: HashMap<InternedString, FunctionDefinition>,
    pub constants: HashMap<InternedString, i32>,
    /// name -> fields (implicit indices)
    pub enums: HashMap<InternedString, Vec<InternedString>>,
    pub structs: HashMap<InternedString, Vec<NamedType>>,
    pub unions: HashMap<InternedString, Vec<NamedType>>,
    pub pragmas: HashMap<InternedString, InternedString>,
}

impl Ast {
    /// Converts JIB AST into BOOM AST
    pub fn from_jib<I: IntoIterator<Item = jib_ast::Definition>>(iter: I) -> Shared<Self> {
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
                .insert("throw".into(), Shared::new(Type::String));
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
            entry_block.set_terminator(control_flow::Terminator::Return(Some(Value::Identifier(
                "return".into(),
            ))));
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
}

/// Top-level definition of a BOOM item
#[derive(Debug, Clone)]
pub enum Definition {
    /// Struct definition
    Struct {
        name: InternedString,
        fields: Vec<NamedType>,
    },

    Union {
        name: InternedString,
        fields: Vec<NamedType>,
    },

    Pragma {
        key: InternedString,
        value: InternedString,
    },
}

impl Walkable for Definition {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        match self {
            Self::Pragma { .. } => (),

            Self::Struct { fields, .. } => {
                fields
                    .iter()
                    .for_each(|named_type| visitor.visit_named_type(named_type));
            }

            Self::Union { fields, .. } => fields
                .iter()
                .for_each(|named_type| visitor.visit_named_type(named_type)),
        }
    }
}

/// Function signature and body
#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    /// Function type signature
    pub signature: FunctionSignature,
    /// Entry block into the control flow graph
    pub entry_block: ControlFlowBlock,
}

impl Walkable for FunctionDefinition {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        visitor.visit_function_signature(&self.signature);
        self.entry_block
            .iter()
            .for_each(|block| visitor.visit_control_flow_block(&block));
    }
}

impl FunctionDefinition {
    /// Gets the type from the type declaration (if it exists) of a local
    /// variable within a function
    pub fn get_ident_type(&self, ident: InternedString) -> Option<Type> {
        // search every statement for ident, should only have a single type declaration,
        // return that type otherwise none
        self.entry_block
            .iter()
            .flat_map(|block| block.statements())
            .filter_map(|statement| {
                if let Statement::VariableDeclaration { name, typ } = &*statement.get() {
                    Some((*name, typ.clone()))
                } else {
                    None
                }
            })
            .chain(
                self.signature
                    .parameters
                    .get()
                    .iter()
                    .map(|Parameter { name, typ, .. }| (*name, typ.clone())),
            )
            .find(|(name, ..)| *name == ident)
            .map(|(.., typ)| typ.get().clone())
    }
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: InternedString,
    pub typ: Shared<Type>,
}

impl Walkable for Parameter {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        visitor.visit_type(self.typ.clone());
    }
}

/// Function parameter and return types
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: InternedString,
    pub parameters: Shared<Vec<Parameter>>,
    pub return_type: Option<Shared<Type>>,
}

impl Walkable for FunctionSignature {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        self.parameters
            .get()
            .iter()
            .for_each(|Parameter { typ, .. }| visitor.visit_type(typ.clone()));

        if let Some(return_type) = &self.return_type {
            visitor.visit_type(return_type.clone());
        }
    }
}

/// Name and type of a union field, struct field, or function parameter
#[derive(Debug, Clone)]
pub struct NamedType {
    pub name: InternedString,
    pub typ: Shared<Type>,
}

impl Walkable for NamedType {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        visitor.visit_type(self.typ.clone());
    }
}

/// Name and type of a union field, struct field, or function parameter
#[derive(Debug, Clone)]
pub struct NamedValue {
    pub name: InternedString,
    pub value: Shared<Value>,
}

impl Walkable for NamedValue {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        visitor.visit_value(self.value.clone());
    }
}

/// Type
#[derive(Debug, Clone)]
pub enum Type {
    // removed before emitting
    Unit,
    String,

    // maybe useful to be distinct?
    Bool,
    Bit,

    Real,
    Float,

    Integer {
        size: Size,
    },
    Bits {
        size: Size,
    },

    Constant(i128),

    Union {
        name: InternedString,
        fields: Vec<NamedType>,
    },

    Struct {
        name: InternedString,
        fields: Vec<NamedType>,
    },

    Tuple(Vec<Shared<Self>>),

    Vector {
        element_type: Shared<Self>,
    },

    FixedVector {
        length: isize,
        element_type: Shared<Self>,
    },

    Reference(Shared<Self>),
}

impl Type {
    // Gets the size of a type
    pub fn get_size(&self) -> Size {
        match self {
            Type::Integer { size } | Type::Bits { size } => size.clone(),
            _ => Size::Unknown,
        }
    }
}

/// Size of a BOOM type in bits
#[derive(Debug, Clone)]
pub enum Size {
    /// Size is known statically at borealis compile time
    Static(usize),
    /// Size is unknown (emitted as uint64)
    Unknown,
}

impl Walkable for Shared<Type> {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        use Type::*;

        match &*self.get() {
            Unit
            | Bool
            | String
            | Real
            | Float
            | Constant(_)
            | Integer { .. }
            | Bits { .. }
            | Bit
            | Union { .. } => {}

            Struct { fields, .. } => fields
                .iter()
                .for_each(|field| visitor.visit_named_type(field)),

            Vector { element_type }
            | FixedVector { element_type, .. }
            | Reference(element_type) => visitor.visit_type(element_type.clone()),

            Tuple(ts) => ts.iter().for_each(|t| visitor.visit_type(t.clone())),
        }
    }
}

impl TryFrom<&Size> for Shared<Value> {
    type Error = ();

    fn try_from(value: &Size) -> Result<Self, Self::Error> {
        match value {
            Size::Static(size) => Ok(Literal::Int((*size).into()).into()),
            Size::Unknown => Err(()),
        }
    }
}

impl TryFrom<Size> for Shared<Value> {
    type Error = ();

    fn try_from(value: Size) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl Add for Size {
    type Output = Size;

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Size::Static(l), Size::Static(r)) => Size::Static(l + r),

            _ => panic!("cannot add unknown"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Statement {
    VariableDeclaration {
        name: InternedString,
        typ: Shared<Type>,
    },
    Copy {
        expression: Expression,
        value: Shared<Value>,
    },
    FunctionCall {
        expression: Option<Expression>, // expressions to write return value(s) to
        name: InternedString,
        arguments: Vec<Shared<Value>>,
    },
    Label(InternedString),
    Goto(InternedString),
    Jump {
        condition: Shared<Value>,
        target: InternedString,
    },
    End(InternedString),
    Undefined,
    If {
        condition: Shared<Value>,
        if_body: Vec<Shared<Statement>>,
        else_body: Vec<Shared<Statement>>,
    },
    Exit(InternedString),
    Comment(InternedString),
    /// Fatal error, printing the supplied values
    Panic(Shared<Value>),
}

impl From<Statement> for Shared<Statement> {
    fn from(value: Statement) -> Self {
        Shared::new(value)
    }
}

impl Walkable for Statement {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        match self {
            Self::VariableDeclaration { typ, .. } => visitor.visit_type(typ.clone()),
            Self::Copy { expression, value } => {
                visitor.visit_expression(expression);
                visitor.visit_value(value.clone());
            }

            Self::FunctionCall {
                expression: expressions,
                arguments,
                ..
            } => {
                if let Some(expression) = expressions {
                    visitor.visit_expression(expression);
                }
                arguments
                    .iter()
                    .for_each(|argument| visitor.visit_value(argument.clone()));
            }
            Self::Label(_) => (),
            Self::Goto(_) => (),
            Self::Jump { condition, .. } => visitor.visit_value(condition.clone()),
            Self::End(_) => (),
            Self::Undefined => (),
            Self::If {
                condition,
                if_body,
                else_body,
            } => {
                visitor.visit_value(condition.clone());
                if_body
                    .iter()
                    .for_each(|statement| visitor.visit_statement(statement.clone()));
                else_body
                    .iter()
                    .for_each(|statement| visitor.visit_statement(statement.clone()));
            }
            Self::Exit(_) => (),
            Self::Comment(_) => (),
            Self::Panic(value) => visitor.visit_value(value.clone()),
        }
    }
}

/// Expression
#[derive(Debug, Clone)]
pub enum Expression {
    Identifier(InternedString),
    Field {
        expression: Box<Self>,
        field: InternedString,
    },
    Address(Box<Self>),
    Tuple(Vec<Self>),
}

impl Walkable for Expression {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        match self {
            Self::Identifier(_) => (),
            Self::Field { expression, .. } | Self::Address(expression) => {
                visitor.visit_expression(expression)
            }
            Self::Tuple(exprs) => exprs.iter().for_each(|e| visitor.visit_expression(e)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Identifier(InternedString),
    Literal(Shared<Literal>),
    Operation(Operation),
    Struct {
        name: InternedString,
        fields: Vec<NamedValue>,
    },
    Field {
        value: Shared<Self>,
        field_name: InternedString,
    },
    CtorKind {
        value: Shared<Self>,
        identifier: InternedString,
        types: Vec<Shared<Type>>,
    },
    CtorUnwrap {
        value: Shared<Self>,
        identifier: InternedString,
        types: Vec<Shared<Type>>,
    },
    Tuple(Vec<Shared<Self>>),
    VectorAccess {
        value: Shared<Self>,
        index: Shared<Self>,
    },
    VectorMutate {
        vector: Shared<Self>,
        element: Shared<Self>,
        index: Shared<Self>,
    },
}

impl Value {
    /// Attempts to evaluate the value of a value as a boolean, returning None
    /// on failure
    pub fn evaluate_bool(&self, ctx: &ControlFlowBlock) -> Option<bool> {
        match &self {
            Self::Identifier(identifier) => {
                let defs = ctx
                    .statements()
                    .iter()
                    .filter_map(|statement| {
                        if let Statement::Copy {
                            expression: Expression::Identifier(target_identifier),
                            value,
                        } = &*statement.get()
                        {
                            if identifier == target_identifier {
                                Some(value.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                // probably a function parameter, or assignment of result of function
                if defs.is_empty() {
                    return None;
                }

                let value = defs.last().unwrap().get();
                value.evaluate_bool(ctx)
            }
            Self::Literal(literal) => match &*literal.get() {
                Literal::Bool(value) => Some(*value),
                _ => None,
            },

            // Self::Operation(op) => op.evaluate_bool(),
            _ => None,
        }
    }

    /// Gets the identifier of the inner variable, if it exists
    pub fn get_ident(&self) -> Option<InternedString> {
        match self {
            Value::Identifier(ident) => Some(*ident),
            Value::Operation(Operation::Not(value)) => value.get().get_ident(),
            _ => None,
        }
    }
}

impl Walkable for Value {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        match self {
            Value::Identifier(_) => (),
            Value::Literal(literal) => visitor.visit_literal(literal.clone()),
            Value::Operation(operation) => visitor.visit_operation(operation),
            Value::Struct { fields, .. } => fields
                .iter()
                .for_each(|field| visitor.visit_named_value(field)),
            Value::Field { value, .. } => visitor.visit_value(value.clone()),
            Value::CtorKind { value, types, .. } | Value::CtorUnwrap { value, types, .. } => {
                visitor.visit_value(value.clone());
                types.iter().for_each(|typ| visitor.visit_type(typ.clone()));
            }
            Value::Tuple(values) => values.iter().for_each(|v| visitor.visit_value(v.clone())),
            Value::VectorAccess { value, index } => {
                visitor.visit_value(value.clone());
                visitor.visit_value(index.clone());
            }
            Value::VectorMutate {
                vector,
                element,
                index,
            } => {
                visitor.visit_value(vector.clone());
                visitor.visit_value(element.clone());
                visitor.visit_value(index.clone());
            }
        }
    }
}

impl From<Literal> for Shared<Value> {
    fn from(value: Literal) -> Self {
        Shared::new(Value::Literal(Shared::new(value)))
    }
}

impl From<Operation> for Shared<Value> {
    fn from(value: Operation) -> Self {
        Shared::new(Value::Operation(value))
    }
}

#[derive(Debug, Clone)]
pub enum Literal {
    Int(BigInt),
    // Little-endian order
    Bits(Vec<Bit>),
    Bit(Bit),
    Bool(bool),
    String(InternedString),
    Vector(Vec<Shared<Literal>>),
    Unit,
    Reference(InternedString),
    Undefined,
}

impl Walkable for Literal {
    fn walk<V: Visitor>(&self, _: &mut V) {
        // leaf node
    }
}

#[derive(Debug, Clone, Kinded)]
pub enum Operation {
    Not(Shared<Value>),
    Complement(Shared<Value>),

    Equal(Shared<Value>, Shared<Value>),
    NotEqual(Shared<Value>, Shared<Value>),

    LessThan(Shared<Value>, Shared<Value>),
    LessThanOrEqual(Shared<Value>, Shared<Value>),
    GreaterThan(Shared<Value>, Shared<Value>),
    GreaterThanOrEqual(Shared<Value>, Shared<Value>),

    Subtract(Shared<Value>, Shared<Value>),
    Add(Shared<Value>, Shared<Value>),
    Or(Shared<Value>, Shared<Value>),
    Multiply(Shared<Value>, Shared<Value>),
    And(Shared<Value>, Shared<Value>),
    Xor(Shared<Value>, Shared<Value>),
    Divide(Shared<Value>, Shared<Value>),

    Cast(Shared<Value>, Shared<Type>),

    LeftShift(Shared<Value>, Shared<Value>),
    RightShift(Shared<Value>, Shared<Value>),
    RotateRight(Shared<Value>, Shared<Value>),
    RotateLeft(Shared<Value>, Shared<Value>),
}

impl Walkable for Operation {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        match self {
            Operation::Not(value) | Operation::Complement(value) => {
                visitor.visit_value(value.clone())
            }
            Operation::Equal(lhs, rhs)
            | Operation::NotEqual(lhs, rhs)
            | Operation::LessThan(lhs, rhs)
            | Operation::GreaterThan(lhs, rhs)
            | Operation::LessThanOrEqual(lhs, rhs)
            | Operation::GreaterThanOrEqual(lhs, rhs)
            | Operation::Subtract(lhs, rhs)
            | Operation::Add(lhs, rhs)
            | Operation::Multiply(lhs, rhs)
            | Operation::Or(lhs, rhs)
            | Operation::Xor(lhs, rhs)
            | Operation::And(lhs, rhs)
            | Operation::Divide(lhs, rhs)
            | Operation::LeftShift(lhs, rhs)
            | Operation::RightShift(lhs, rhs)
            | Operation::RotateLeft(lhs, rhs)
            | Operation::RotateRight(lhs, rhs) => {
                visitor.visit_value(lhs.clone());
                visitor.visit_value(rhs.clone());
            }
            Operation::Cast(value, typ) => {
                visitor.visit_value(value.clone());
                visitor.visit_type(typ.clone());
            }
        }
    }
}

/// Bit
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Bit {
    /// Fixed zero
    Zero,
    /// Fixed one
    One,
    /// Unknown bit
    Unknown,
}

impl Bit {
    pub fn is_unknown(&self) -> bool {
        match self {
            Self::Zero | Self::One => false,
            Self::Unknown => true,
        }
    }

    pub fn is_fixed(&self) -> bool {
        !self.is_unknown()
    }

    /// Gets the value of the bit, panicking if unknown
    pub fn value(&self) -> u64 {
        match self {
            Bit::Zero => 0,
            Bit::One => 1,
            Bit::Unknown => panic!("unknown bit has no value"),
        }
    }
}

impl From<u64> for Bit {
    fn from(value: u64) -> Self {
        match value {
            0 => Bit::Zero,
            1 => Bit::One,
            _ => panic!("value must be 0 or 1 to be interpreted as a bit"),
        }
    }
}

impl Debug for Bit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Zero => write!(f, "0"),
            Self::One => write!(f, "1"),
            Self::Unknown => write!(f, "x"),
        }
    }
}

/// Converts a sequence of bits to an integer
pub fn bits_to_int<B: AsRef<[Bit]>>(bits: B) -> u64 {
    let bits = bits.as_ref();

    assert!(bits.iter().all(Bit::is_fixed));

    bits.iter().rev().fold(0, |acc, bit| acc << 1 | bit.value())
}
