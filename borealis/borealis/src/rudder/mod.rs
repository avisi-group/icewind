use {
    crate::rudder::{
        constant_value::ConstantValue,
        statement::{Statement, StatementKind},
    },
    common::{
        intern::InternedString,
        shared::{Shared, Weak},
        HashMap, HashSet,
    },
    log::warn,
    proc_macro2::TokenStream,
    quote::ToTokens,
    std::{
        fmt::Debug,
        hash::{Hash, Hasher},
        sync::Arc,
    },
};

pub mod analysis;
pub mod build;
pub mod constant_value;
pub mod internal_fns;
pub mod opt;
mod pretty_print;
pub mod statement;
pub mod validator;

#[derive(Debug, Hash, Clone, Copy, Eq, PartialEq)]
pub enum PrimitiveTypeClass {
    Void,
    Unit,
    UnsignedInteger,
    SignedInteger,
    FloatingPoint,
}

#[derive(Debug, Hash, Clone, Eq, PartialEq)]
pub struct PrimitiveType {
    pub tc: PrimitiveTypeClass,
    pub element_width_in_bits: usize,
}

impl PrimitiveType {
    pub fn type_class(&self) -> PrimitiveTypeClass {
        self.tc
    }

    pub fn width(&self) -> usize {
        self.element_width_in_bits
    }
}

#[derive(Debug, Hash, Clone, Eq, PartialEq)]
pub enum Type {
    Primitive(PrimitiveType),
    Product(Vec<(InternedString, Arc<Type>)>),
    Sum(Vec<(InternedString, Arc<Type>)>),
    Vector {
        element_count: usize,
        element_type: Arc<Type>,
    },

    // ehhhh
    String,

    Bits,
    ArbitraryLengthInteger,
    Rational,

    // Any type, used for undefineds
    Any,
}

macro_rules! type_def_helper {
    ($name: ident, $cls: ident, $width: expr) => {
        pub fn $name() -> Self {
            Self::new_primitive(PrimitiveTypeClass::$cls, $width)
        }
    };
}

impl Type {
    pub fn new_primitive(tc: PrimitiveTypeClass, element_width: usize) -> Self {
        Self::Primitive(PrimitiveType {
            tc,
            element_width_in_bits: element_width,
        })
    }

    pub fn new_product(fields: Vec<(InternedString, Arc<Type>)>) -> Self {
        Self::Product(fields)
    }

    pub fn new_sum(variants: Vec<(InternedString, Arc<Type>)>) -> Self {
        Self::Sum(variants)
    }

    pub fn void() -> Self {
        Self::Primitive(PrimitiveType {
            tc: PrimitiveTypeClass::Void,
            element_width_in_bits: 0,
        })
    }

    pub fn unit() -> Self {
        Self::Primitive(PrimitiveType {
            tc: PrimitiveTypeClass::Unit,
            element_width_in_bits: 0,
        })
    }

    /// Gets the offset in bytes of a field of a composite or an element of a
    /// vector
    pub fn byte_offset(&self, element_field: usize) -> Option<usize> {
        match self {
            Type::Product(fields) => Some(
                fields
                    .iter()
                    .take(element_field)
                    .fold(0, |acc, (_, typ)| acc + typ.width_bytes()),
            ),
            Type::Sum(_) => Some(0),
            Type::Vector { element_type, .. } => Some(element_field * element_type.width_bytes()),
            _ => None,
        }
    }

    pub fn width_bits(&self) -> usize {
        match self {
            Self::Product(xs) => xs.iter().map(|(_, typ)| typ.width_bits()).sum(),
            Self::Sum(xs) => xs.iter().map(|(_, typ)| typ.width_bits()).max().unwrap(),
            // smallest with is 8 bits
            Self::Primitive(p) => p.element_width_in_bits.max(8),
            Self::Vector {
                element_count,
                element_type,
            } => element_type.width_bits() * element_count,

            Self::Bits | Self::ArbitraryLengthInteger => usize::try_from(u128::BITS).unwrap(),
            // width of internedstring
            Self::String => 32,
            Self::Rational => todo!(),
            Self::Any => todo!(),
        }
    }

    pub fn width_bytes(&self) -> usize {
        self.width_bits().div_ceil(8)
    }

    type_def_helper!(u1, UnsignedInteger, 1);
    type_def_helper!(u8, UnsignedInteger, 8);
    type_def_helper!(u16, UnsignedInteger, 16);
    type_def_helper!(u32, UnsignedInteger, 32);
    type_def_helper!(u64, UnsignedInteger, 64);
    type_def_helper!(u128, UnsignedInteger, 128);
    type_def_helper!(s8, SignedInteger, 8);
    type_def_helper!(s16, SignedInteger, 16);
    type_def_helper!(s32, SignedInteger, 32);
    type_def_helper!(s64, SignedInteger, 64);
    type_def_helper!(s128, SignedInteger, 128);
    type_def_helper!(f32, FloatingPoint, 32);
    type_def_helper!(f64, FloatingPoint, 64);

    pub fn vectorize(self, element_count: usize) -> Self {
        Self::Vector {
            element_count,
            element_type: Arc::new(self),
        }
    }

    pub fn is_void(&self) -> bool {
        match self {
            Self::Primitive(PrimitiveType { tc, .. }) => {
                matches!(tc, PrimitiveTypeClass::Void)
            }
            _ => false,
        }
    }

    pub fn is_unit(&self) -> bool {
        match self {
            Self::Primitive(PrimitiveType { tc, .. }) => {
                matches!(tc, PrimitiveTypeClass::Unit)
            }
            _ => false,
        }
    }

    pub fn is_bits(&self) -> bool {
        matches!(self, Self::Bits)
    }

    pub fn is_u1(&self) -> bool {
        match self {
            Self::Primitive(PrimitiveType {
                tc: PrimitiveTypeClass::UnsignedInteger,
                element_width_in_bits,
            }) => *element_width_in_bits == 1,
            _ => false,
        }
    }

    pub fn is_unknown_length_vector(&self) -> bool {
        matches!(
            self,
            Self::Vector {
                element_count: 0,
                ..
            }
        )
    }

    pub fn is_compatible_with(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum SymbolKind {
    Parameter,
    LocalVariable,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    name: InternedString,
    kind: SymbolKind,
    typ: Arc<Type>,
}

impl Symbol {
    pub fn name(&self) -> InternedString {
        self.name
    }

    pub fn kind(&self) -> SymbolKind {
        self.kind
    }

    pub fn typ(&self) -> Arc<Type> {
        self.typ.clone()
    }
}

#[derive(Clone)]
pub struct Block {
    inner: Shared<BlockInner>,
}

impl Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner.get().name)
    }
}

#[derive(Debug, Clone)]
pub struct WeakBlock {
    inner: Weak<BlockInner>,
}

impl WeakBlock {
    pub fn upgrade(&self) -> Block {
        Block {
            inner: self.inner.upgrade().unwrap(),
        }
    }
}

impl Block {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn weak(&self) -> WeakBlock {
        WeakBlock {
            inner: self.inner.downgrade(),
        }
    }

    pub fn name(&self) -> InternedString {
        self.inner.get().name
    }

    pub fn update_names(&self, name: InternedString) {
        self.inner.get_mut().update_names(name);
    }

    pub fn statements(&self) -> Vec<Statement> {
        self.inner.get().statements.clone()
    }

    pub fn terminator_statement(&self) -> Option<Statement> {
        self.inner.get().statements.last().cloned()
    }

    pub fn set_statements<I: Iterator<Item = Statement>>(&self, statements: I) {
        self.inner.get_mut().statements = statements.collect();
    }

    pub fn extend_statements<I: Iterator<Item = Statement>>(&self, stmts: I) {
        self.inner.get_mut().statements.extend(stmts)
    }

    fn index_of_statement(&self, reference: &Statement) -> usize {
        self.inner
            .get()
            .statements
            .iter()
            .enumerate()
            .find(|(_, candidate)| *candidate == reference)
            .unwrap()
            .0
    }

    pub fn insert_statement_before(&self, reference: &Statement, new: Statement) {
        let index = self.index_of_statement(reference);
        self.inner.get_mut().statements.insert(index, new);
    }

    pub fn append_statement(&self, new: Statement) {
        self.inner.get_mut().statements.push(new);
    }

    pub fn kill_statement(&self, stmt: &Statement) {
        //assert!(Rc::ptr_eq()

        let index = self.index_of_statement(stmt);

        self.inner.get_mut().statements.remove(index);
    }

    pub fn iter(&self) -> BlockIterator {
        BlockIterator::new(self.clone())
    }

    pub fn targets(&self) -> Vec<Block> {
        match self.terminator_statement().unwrap().kind() {
            StatementKind::Jump { target } => vec![target],
            StatementKind::Branch {
                true_target,
                false_target,
                ..
            } => vec![true_target, false_target],
            StatementKind::Return { .. }
            | StatementKind::Panic(_)
            | StatementKind::Call { tail: true, .. } => vec![],
            _ => panic!("invalid terminator for block"),
        }
    }

    pub fn size(&self) -> usize {
        self.statements().len()
    }
}

impl Default for Block {
    fn default() -> Self {
        Self {
            inner: Shared::new(BlockInner {
                name: "???".into(),
                statements: Vec::new(),
            }),
        }
    }
}

impl Hash for Block {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::ptr::hash(self.inner.as_ptr(), state)
    }
}

impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        Shared::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for Block {}

#[derive(Debug)]
pub struct BlockInner {
    name: InternedString,
    statements: Vec<Statement>,
}

impl BlockInner {
    pub fn update_names(&mut self, name: InternedString) {
        self.name = name;

        self.statements.iter().enumerate().for_each(|(idx, stmt)| {
            stmt.update_names(format!("s_{}_{}", name.clone(), idx).into());
        });
    }
}

pub struct BlockIterator {
    visited: HashSet<Block>,
    remaining: Vec<Block>,
}

impl BlockIterator {
    fn new(start_block: Block) -> Self {
        Self {
            visited: HashSet::default(),
            remaining: vec![start_block],
        }
    }
}

impl Iterator for BlockIterator {
    type Item = Block;

    fn next(&mut self) -> Option<Self::Item> {
        let current = loop {
            let Some(current) = self.remaining.pop() else {
                // if remaining is empty, all blocks have been visited
                return None;
            };

            // if we've already visited the node, get the next one
            if self.visited.contains(&current) {
                continue;
            } else {
                // otherwise return it
                break current;
            }
        };

        // mark current node as processed
        self.visited.insert(current.clone());

        // push children to visit
        if let Some(last) = current.statements().last() {
            self.remaining.extend(match last.kind() {
                StatementKind::Jump { target } => vec![target.clone()],
                StatementKind::Branch {
                    true_target,
                    false_target,
                    ..
                } => vec![true_target.clone(), false_target.clone()],
                StatementKind::Return { .. }
                | StatementKind::Panic(_)
                | StatementKind::Call { tail: true, .. } => vec![],
                _ => {
                    warn!("block missing terminator");
                    vec![]
                }
            })
        }

        Some(current)
    }
}

#[derive(Clone)]
pub struct Function {
    inner: Shared<FunctionInner>,
}

impl Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner.get().name)
    }
}

impl ToTokens for Function {
    fn to_tokens(&self, _: &mut TokenStream) {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct FunctionInner {
    name: InternedString,
    return_type: Arc<Type>,
    parameters: Vec<Symbol>,
    local_variables: HashMap<InternedString, Symbol>,
    entry_block: Block,
}

impl Function {
    pub fn new<I: Iterator<Item = (InternedString, Arc<Type>)>>(
        name: InternedString,
        return_type: Arc<Type>,
        parameters: I,
    ) -> Self {
        let mut celf = Self {
            inner: Shared::new(FunctionInner {
                name,
                return_type: return_type.clone(),
                parameters: parameters
                    .map(|(name, typ)| Symbol {
                        name,
                        kind: SymbolKind::Parameter,
                        typ,
                    })
                    .collect(),
                local_variables: HashMap::default(),
                entry_block: Block::new(),
            }),
        };

        if return_type.is_void() {
            panic!("functions must have a return type (unit not void)");
        }

        celf.add_local_variable("return_value".into(), return_type);

        celf
    }

    pub fn name(&self) -> InternedString {
        self.inner.get().name
    }

    pub fn weight(&self) -> u64 {
        0 //self.inner.borrow().entry_block().iter().
    }

    pub fn signature(&self) -> (Arc<Type>, Vec<Symbol>) {
        (
            self.inner.get().return_type.clone(),
            self.inner.get().parameters.clone(),
        )
    }

    pub fn update_names(&self) {
        self.inner
            .get()
            .entry_block
            .iter()
            .enumerate()
            .for_each(|(idx, b)| {
                b.update_names(format!("{idx}").into());
            });
    }

    pub fn add_local_variable(&mut self, name: InternedString, typ: Arc<Type>) {
        self.inner.get_mut().local_variables.insert(
            name,
            Symbol {
                name,
                kind: SymbolKind::LocalVariable,
                typ,
            },
        );
    }

    pub fn get_local_variable(&self, name: InternedString) -> Option<Symbol> {
        self.inner.get().local_variables.get(&name).cloned()
    }

    pub fn local_variables(&self) -> Vec<Symbol> {
        self.inner.get().local_variables.values().cloned().collect()
    }

    pub fn remove_local_variable(&self, symbol: &Symbol) {
        self.inner.get_mut().local_variables.remove(&symbol.name());
    }

    pub fn get_parameter(&self, name: InternedString) -> Option<Symbol> {
        self.inner
            .get()
            .parameters
            .iter()
            .find(|sym| sym.name() == name)
            .cloned()
    }

    pub fn return_type(&self) -> Arc<Type> {
        self.inner.get().return_type.clone()
    }

    pub fn parameters(&self) -> Vec<Symbol> {
        self.inner.get().parameters.clone()
    }

    pub fn entry_block(&self) -> Block {
        self.inner.get().entry_block.clone()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum FunctionKind {
    Execute,
    Other,
}

#[derive(Default)]
pub struct Context {
    fns: HashMap<InternedString, (FunctionKind, Function)>,
    // offset-type pairs, offsets may not be unique? todo: ask tom
    registers: HashMap<InternedString, RegisterDescriptor>,
    structs: HashSet<Arc<Type>>,
    unions: HashSet<Arc<Type>>,
}

#[derive(Clone, Debug)]
pub struct RegisterDescriptor {
    pub typ: Arc<Type>,
    pub offset: usize,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_function(&mut self, name: InternedString, kind: FunctionKind, func: Function) {
        self.fns.insert(name, (kind, func));
    }

    pub fn update_names(&self) {
        for (_, func) in self.fns.values() {
            func.update_names();
        }
    }

    pub fn optimise(&mut self) {
        opt::optimise(self);
    }

    pub fn validate(&mut self) -> Vec<validator::ValidationMessage> {
        validator::validate(self)
    }

    pub fn get_functions(&self) -> HashMap<InternedString, Function> {
        self.fns
            .iter()
            .map(|(name, (_, function))| (*name, function.clone()))
            .collect()
    }

    pub fn get_registers(&self) -> HashMap<InternedString, RegisterDescriptor> {
        self.registers.clone()
    }

    pub fn get_structs(&self) -> HashSet<Arc<Type>> {
        self.structs.clone()
    }

    pub fn get_unions(&self) -> HashSet<Arc<Type>> {
        self.unions.clone()
    }
}
