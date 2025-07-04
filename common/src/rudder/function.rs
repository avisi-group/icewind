use {
    crate::{
        arena::{Arena, Ref},
        hashmap::HashMap,
        intern::InternedString,
        rudder::{
            block::{Block, BlockIterator},
            types::Type,
        },
    },
    alloc::vec::Vec,
    core::fmt::{self, Debug, Display, Formatter},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Symbol {
    name: InternedString,
    typ: Type,
}

impl Symbol {
    pub fn new(name: InternedString, typ: Type) -> Self {
        Self { name, typ }
    }

    pub fn name(&self) -> InternedString {
        self.name
    }

    pub fn typ(&self) -> Type {
        self.typ.clone()
    }
}

impl Display for Symbol {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Function {
    // return type and parameters are read only, so do not need to exist behind a `Shared`
    return_type: Option<Type>,
    parameters: Vec<Symbol>,
    name: InternedString,
    local_variables: HashMap<InternedString, Symbol>,
    block_arena: Arena<Block>,
    entry_block: Ref<Block>,
}

impl Debug for Function {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Function {
    pub fn new(name: InternedString, return_type: Option<Type>, parameters: Vec<Symbol>) -> Self {
        let mut block_arena = Arena::new();
        let entry_block = block_arena.insert(Block::new());
        Self {
            name,
            local_variables: HashMap::default(),
            entry_block,
            block_arena,
            return_type,
            parameters,
        }
    }

    pub fn name(&self) -> InternedString {
        self.name
    }

    pub fn add_local_variable(&mut self, symbol: Symbol) {
        self.local_variables.insert(symbol.name(), symbol);
    }

    pub fn get_local_variable(&self, name: InternedString) -> Option<Symbol> {
        self.local_variables.get(&name).cloned()
    }

    pub fn local_variables(&self) -> Vec<Symbol> {
        self.local_variables.values().cloned().collect()
    }

    pub fn remove_local_variable(&mut self, symbol: &Symbol) {
        self.local_variables.remove(&symbol.name());
    }

    pub fn get_parameter(&self, name: InternedString) -> Option<Symbol> {
        self.parameters
            .iter()
            .find(|sym| sym.name() == name)
            .cloned()
    }

    pub fn return_type(&self) -> Option<Type> {
        self.return_type.clone()
    }

    pub fn parameters(&self) -> Vec<Symbol> {
        self.parameters.clone()
    }

    pub fn remove_parameter(&mut self, symbol: &Symbol) {
        let idx = self
            .parameters
            .iter()
            .enumerate()
            .find(|(_, target)| target.name() == symbol.name())
            .map(|(i, _)| i)
            .unwrap();

        self.parameters.remove(idx);
    }

    pub fn new_block(&mut self) -> Ref<Block> {
        self.block_arena.insert(Block::new())
    }

    pub fn arena(&self) -> &Arena<Block> {
        &self.block_arena
    }

    pub fn arena_mut(&mut self) -> &mut Arena<Block> {
        &mut self.block_arena
    }

    pub fn entry_block(&self) -> Ref<Block> {
        self.entry_block
    }

    pub fn set_entry_block(&mut self, b: Ref<Block>) {
        self.entry_block = b;
    }

    pub fn block_iter(&self) -> BlockIterator<'_> {
        BlockIterator::new(self.arena(), self.entry_block)
    }
}
