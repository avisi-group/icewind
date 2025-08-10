//! Generating control flow graph from BOOM
//!
//! Needs to be restructured due to two main areas of complexity, both caused by
//! block targets being unresolved during building (IE. visiting a jump before
//! the label it references is created):
//!
//! 1. Two sets of types, internal maybe-unresolved and public resolved to
//! enforce resolution at type level. 2. Recursive resolution to convert
//! maybe-unresolved to resolved blocks.

use {
    crate::boom::{
        Literal, Statement, Value,
        visitor::{Visitor, Walkable},
    },
    common::{hashmap::HashSet, id::Id, intern::InternedString},
    itertools::Itertools,
    sailrs::shared::{Shared, Weak},
    std::{
        fmt::{self, Display, Formatter},
        hash::{DefaultHasher, Hash, Hasher},
        ptr,
    },
};

pub mod dot;
pub mod util;

/// Node in a control flow graph, contains a basic block of statements and a
/// terminator
#[derive(Debug, Clone, Default)]
pub struct ControlFlowBlock {
    inner: Shared<ControlFlowBlockInner>,
}

#[derive(Debug, Default)]
struct ControlFlowBlockInner {
    /// Unique block ID
    id: Id,
    /// Optional block label, otherwise the `SharedKey` `Display` format should
    /// be used
    label: Option<InternedString>,
    /// Parents of the current node
    parents: HashSet<ControlFlowBlockWeak>,
    /// Sequence of statements within the block
    statements: Vec<Shared<Statement>>,
    /// Block terminator
    terminator: Terminator,
}

impl Display for ControlFlowBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.inner.get().label {
            Some(label) => write!(f, "{label}"),
            None => {
                let mut state = DefaultHasher::new();
                self.id().hash(&mut state);
                write!(f, "{:016X}", state.finish())
            }
        }
    }
}

impl Walkable for ControlFlowBlock {
    fn walk<V: Visitor>(&self, visitor: &mut V) {
        self.statements()
            .iter()
            .cloned()
            .for_each(|statement| visitor.visit_statement(statement));
    }
}

impl ControlFlowBlock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(&self) -> Id {
        self.inner.get().id
    }

    /// Creates a new weak pointer to a `ControlFlowBlock`
    pub fn downgrade(&self) -> ControlFlowBlockWeak {
        ControlFlowBlockWeak(self.inner.downgrade())
    }

    /// Gets the label of a `ControlFlowBlock`
    pub fn label(&self) -> Option<InternedString> {
        self.inner.get().label
    }

    /// Sets the label of a `ControlFlowBlock`
    pub fn set_label(&self, label: Option<InternedString>) {
        self.inner.get_mut().label = label;
    }

    /// Gets the parents of this control flow block
    pub fn parents(&self) -> Vec<ControlFlowBlock> {
        self.inner
            .get()
            .parents
            .iter()
            .filter_map(ControlFlowBlockWeak::upgrade)
            .collect()
    }

    /// Adds a parent to this control flow block
    pub fn add_parent(&self, parent: &Self) {
        let parents = &mut self.inner.get_mut().parents;

        // remove dropped weak pointers
        parents.retain(|weak| weak.upgrade().is_some());

        // add new weak pointer
        parents.insert(parent.downgrade());
    }

    fn remove_parent(&self, parent_to_remove: &Self) {
        let parent_to_remove = parent_to_remove.downgrade();
        let parents = &mut self.inner.get_mut().parents;

        if !parents.contains(&parent_to_remove) {
            panic!(
                "parent {} not found in parents",
                parent_to_remove.upgrade().unwrap(),
            )
        }

        parents.retain(|p| *p != parent_to_remove);
    }

    /// Gets the terminator of this control flow block
    pub fn terminator(&self) -> Terminator {
        self.inner.get().terminator.clone()
    }

    /// Sets the terminator of this control flow block (and also the weak
    /// parental references of any children)
    pub fn set_terminator(&self, terminator: Terminator) {
        // remove ourselves as a parent from all children
        self.terminator()
            .targets()
            .iter()
            .dedup_by(|x, y| x.id() == y.id()) // sequential dedup only, but any duplicates in conditional targets will be sequential
            // anyway
            .for_each(|child| child.remove_parent(self));

        match &terminator {
            Terminator::Return(_) | Terminator::Panic(_) => (),
            Terminator::Conditional {
                target,
                fallthrough,
                ..
            } => {
                target.add_parent(self);
                fallthrough.add_parent(self);
            }
            Terminator::Unconditional { target } => target.add_parent(self),
        }

        self.inner.get_mut().terminator = terminator;
    }

    /// Gets the statements of a `ControlFlowBlock`
    pub fn statements(&self) -> Vec<Shared<Statement>> {
        self.inner.get().statements.clone()
    }

    /// Sets the statements of a `ControlFlowBlock`
    pub fn set_statements(&self, statements: Vec<Shared<Statement>>) {
        self.inner.get_mut().statements = statements;
    }

    pub fn iter(&self) -> ControlFlowBlockIterator {
        ControlFlowBlockIterator::new(self.clone())
    }
}

/// Non-owning reference to a `ControlFlowBlock`
#[derive(Debug)]
pub struct ControlFlowBlockWeak(Weak<ControlFlowBlockInner>);

impl ControlFlowBlockWeak {
    /// Attempts to upgrade to a strong reference to a `ControlFlowBlock`
    pub fn upgrade(&self) -> Option<ControlFlowBlock> {
        self.0.upgrade().map(|inner| ControlFlowBlock { inner })
    }
}

impl Hash for ControlFlowBlockWeak {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ptr::hash(self.0.as_ptr(), state)
    }
}

impl PartialEq for ControlFlowBlockWeak {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ControlFlowBlockWeak {}

/// Block terminator
///
/// Describes how one block conditionally or unconditionally jumps to the next
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Function return with value
    Return(Option<Value>),
    Panic(Value),
    /// If condition evaluates to true, then jump to target, otherwise jump to
    /// fallthrough
    Conditional {
        /// Condition
        condition: Value,
        /// Target if condition is `true`
        target: ControlFlowBlock,
        /// Fallthrough if condition is `false`
        fallthrough: ControlFlowBlock,
    },
    /// Unconditionally jump to target
    Unconditional {
        /// Target to branch to
        target: ControlFlowBlock,
    },
}

impl Default for Terminator {
    fn default() -> Self {
        Self::Panic(Value::Literal(Shared::new(Literal::String(
            "default terminator".into(),
        ))))
    }
}

impl Terminator {
    /// Gets the targets of a terminator
    pub fn targets(&self) -> Vec<ControlFlowBlock> {
        match self {
            Terminator::Return(_) | Terminator::Panic(_) => vec![],
            Terminator::Conditional {
                target,
                fallthrough,
                ..
            } => vec![target.clone(), fallthrough.clone()],
            Terminator::Unconditional { target } => vec![target.clone()],
        }
    }
}

#[derive(Debug)]
pub struct ControlFlowBlockIterator {
    visited: HashSet<Id>,
    remaining: Vec<ControlFlowBlock>,
}

impl ControlFlowBlockIterator {
    fn new(start_block: ControlFlowBlock) -> Self {
        Self {
            visited: HashSet::default(),
            remaining: vec![start_block],
        }
    }
}

impl Iterator for ControlFlowBlockIterator {
    type Item = ControlFlowBlock;

    fn next(&mut self) -> Option<Self::Item> {
        let current = loop {
            let Some(current) = self.remaining.pop() else {
                // if remaining is empty, all blocks have been visited
                return None;
            };

            // if we've already visited the node, get the next one
            if self.visited.contains(&current.id()) {
                continue;
            } else {
                // otherwise return it
                break current;
            }
        };

        // mark current node as processed
        self.visited.insert(current.id());

        // push children to visit
        self.remaining.extend(current.terminator().targets());

        Some(current)
    }
}
