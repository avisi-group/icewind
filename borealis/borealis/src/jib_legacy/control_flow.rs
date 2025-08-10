use {
    crate::boom::{
        Literal, Statement, Value,
        control_flow::{ControlFlowBlock, Terminator},
    },
    common::{intern::InternedString, hashmap::HashMap},
    sailrs::shared::{Shared, SharedKey},
};

/// Builder structure for a control flow graph
///
/// Contains state required to build the control flow graph, resolving labels
/// and block terminators
#[derive(Debug)]
pub struct ControlFlowGraphBuilder {
    labels: HashMap<InternedString, Shared<MaybeUnresolvedControlFlowBlock>>,
    resolved_blocks: HashMap<SharedKey<MaybeUnresolvedControlFlowBlock>, ControlFlowBlock>,
    entry_block: Shared<MaybeUnresolvedControlFlowBlock>,
    current_block: Shared<MaybeUnresolvedControlFlowBlock>,
}

impl ControlFlowGraphBuilder {
    pub fn from_statements(statements: &[Shared<Statement>]) -> ControlFlowBlock {
        let entry_block = MaybeUnresolvedControlFlowBlock::new();

        let mut celf = Self {
            labels: Default::default(),
            resolved_blocks: Default::default(),
            current_block: entry_block.clone(),
            entry_block,
        };

        celf.process_statements(statements);

        celf.resolve()
    }

    fn process_statements(&mut self, statements: &[Shared<Statement>]) {
        for statement in statements {
            match &*statement.get() {
                Statement::Label(label) => {
                    let next = MaybeUnresolvedControlFlowBlock::new();

                    self.labels.insert(*label, next.clone());

                    next.get_mut().label = Some(*label);

                    self.current_block.get_mut().set_terminator(
                        MaybeUnresolvedTerminator::Unconditional(
                            MaybeUnresolvedJumpTarget::Resolved {
                                target: next.clone(),
                            },
                        ),
                    );

                    self.current_block = next;
                }
                Statement::If {
                    if_body,
                    else_body,
                    condition,
                } => {
                    let if_block = MaybeUnresolvedControlFlowBlock::new();
                    let else_block = MaybeUnresolvedControlFlowBlock::new();
                    let post_block = MaybeUnresolvedControlFlowBlock::new();

                    self.current_block.get_mut().set_terminator(
                        MaybeUnresolvedTerminator::Conditional {
                            condition: condition.get().clone(),
                            target: MaybeUnresolvedJumpTarget::Resolved {
                                target: if_block.clone(),
                            },
                            fallthrough: MaybeUnresolvedJumpTarget::Resolved {
                                target: else_block.clone(),
                            },
                        },
                    );

                    self.current_block = if_block;
                    self.process_statements(if_body.as_slice());
                    self.current_block.get_mut().set_terminator(
                        MaybeUnresolvedTerminator::Unconditional(
                            MaybeUnresolvedJumpTarget::Resolved {
                                target: post_block.clone(),
                            },
                        ),
                    );

                    self.current_block = else_block;
                    self.process_statements(else_body);
                    self.current_block.get_mut().set_terminator(
                        MaybeUnresolvedTerminator::Unconditional(
                            MaybeUnresolvedJumpTarget::Resolved {
                                target: post_block.clone(),
                            },
                        ),
                    );

                    self.current_block = post_block;
                }
                Statement::Jump { target, condition } => {
                    let fallthrough_block = MaybeUnresolvedControlFlowBlock::new();
                    self.current_block.get_mut().set_terminator(
                        MaybeUnresolvedTerminator::Conditional {
                            condition: condition.get().clone(),
                            target: MaybeUnresolvedJumpTarget::Unresolved { label: *target },
                            fallthrough: MaybeUnresolvedJumpTarget::Resolved {
                                target: fallthrough_block.clone(),
                            },
                        },
                    );

                    self.current_block = fallthrough_block;
                }
                Statement::Goto(label) => {
                    // end current block
                    self.current_block.get_mut().set_terminator(
                        MaybeUnresolvedTerminator::Unconditional(
                            MaybeUnresolvedJumpTarget::Unresolved { label: *label },
                        ),
                    );

                    // start new, "detached" block
                    self.current_block = MaybeUnresolvedControlFlowBlock::new();
                }
                Statement::End(ident) => {
                    // end current block
                    self.current_block
                        .get_mut()
                        .set_terminator(MaybeUnresolvedTerminator::Return(*ident));

                    // start new, "detached" block
                    self.current_block = MaybeUnresolvedControlFlowBlock::new();
                }
                Statement::Undefined => {
                    // end current block
                    self.current_block
                        .get_mut()
                        .set_terminator(MaybeUnresolvedTerminator::Undefined);

                    // start new, "detached" block
                    self.current_block = MaybeUnresolvedControlFlowBlock::new();
                }
                Statement::Panic(value) => {
                    // end current block
                    self.current_block
                        .get_mut()
                        .set_terminator(MaybeUnresolvedTerminator::Panic(value.get().clone()));

                    // start new, "detached" block
                    self.current_block = MaybeUnresolvedControlFlowBlock::new();
                }

                _ => self.current_block.get_mut().add_statement(statement),
            }
        }
    }

    /// Converts unresolved blocks into resolved blocks, errors if any target
    /// labels were not present in the labels map
    pub fn resolve(mut self) -> ControlFlowBlock {
        self.resolve_block(self.entry_block.clone())
    }

    fn resolve_block(
        &mut self,
        unresolved: Shared<MaybeUnresolvedControlFlowBlock>,
    ) -> ControlFlowBlock {
        // if block is already resolved, return that
        if let Some(resolved) = self
            .resolved_blocks
            .get(&SharedKey::from(unresolved.clone()))
        {
            return resolved.clone();
        }

        // create a new control flow block
        let resolved = ControlFlowBlock::new();

        // insert the new resolved control flow block into the map, panicking if it was
        // already resolved
        //
        // it's not populated with the correct statements or terminator at this point,
        // but because it's an Rc-RefCell we can mutate it without modifying the
        // map.
        //
        // we need to insert it here so that the recursive calls as part of the
        // terminator resolution can acquire a reference to the resolved block,
        // if we inserted the correctly resolved block after the recursive call,
        // it would loop forever.
        if let Some(block) = self
            .resolved_blocks
            .insert(unresolved.clone().into(), resolved.clone())
        {
            panic!(
                "unresolved control flow block {unresolved:?} already resolved {block:?} when inserting {resolved:?}"
            )
        }

        resolved.set_label(unresolved.get().label);

        // clone the statements across
        resolved.set_statements(unresolved.get().statements.clone());

        // resolve each kind of terminator
        let terminator = match &unresolved.get().terminator {
            MaybeUnresolvedTerminator::Return(ident) => {
                Terminator::Return(Some(Value::Identifier(*ident)))
            }
            MaybeUnresolvedTerminator::Undefined => Terminator::Panic(Value::Literal(Shared::new(
                Literal::String("undefined terminator".into()),
            ))),
            MaybeUnresolvedTerminator::Panic(value) => Terminator::Panic(value.clone()),
            MaybeUnresolvedTerminator::Conditional {
                condition,
                target,
                fallthrough,
            } => Terminator::Conditional {
                condition: condition.clone(),
                target: self.resolve_jump_target(target),
                fallthrough: self.resolve_jump_target(fallthrough),
            },
            MaybeUnresolvedTerminator::Unconditional(target) => Terminator::Unconditional {
                target: self.resolve_jump_target(target),
            },

            MaybeUnresolvedTerminator::Unknown => {
                panic!("encountered unknown terminator during resolution\n{self:#?}")
            }
        };
        ControlFlowBlock::set_terminator(&resolved, terminator);

        resolved
    }

    fn resolve_jump_target(&mut self, target: &MaybeUnresolvedJumpTarget) -> ControlFlowBlock {
        let block = match target {
            MaybeUnresolvedJumpTarget::Resolved { target } => target.clone(),
            MaybeUnresolvedJumpTarget::Unresolved { label } => self
                .labels
                .get(label)
                .cloned()
                .unwrap_or_else(|| panic!("no entry in label map found for {label:?}")),
        };

        self.resolve_block(block)
    }
}

#[derive(Debug, Clone, Default)]
struct MaybeUnresolvedControlFlowBlock {
    label: Option<InternedString>,
    statements: Vec<Shared<Statement>>,
    terminator: MaybeUnresolvedTerminator,
}

impl MaybeUnresolvedControlFlowBlock {
    fn new() -> Shared<Self> {
        Shared::new(Self::default())
    }

    fn add_statement(&mut self, statement: &Shared<Statement>) {
        assert!(
            !self.has_terminator(),
            "attempted to add statement to block with terminator"
        );
        self.statements.push(statement.clone());
    }

    fn has_terminator(&self) -> bool {
        !matches!(self.terminator, MaybeUnresolvedTerminator::Unknown)
    }

    fn set_terminator(&mut self, terminator: MaybeUnresolvedTerminator) {
        if self.has_terminator() {
            panic!(
                "attempted to set terminator to block with terminator: \n{self:#?}\n{terminator:#?}"
            )
        } else {
            self.terminator = terminator;
        }
    }
}

/// Possibly-unresolved block terminator statement
#[derive(Debug, Clone)]
enum MaybeUnresolvedTerminator {
    Return(InternedString),
    Conditional {
        condition: Value,
        target: MaybeUnresolvedJumpTarget,
        fallthrough: MaybeUnresolvedJumpTarget,
    },
    Unconditional(MaybeUnresolvedJumpTarget),
    Undefined,
    Unknown,
    Panic(Value),
}

impl Default for MaybeUnresolvedTerminator {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone)]
enum MaybeUnresolvedJumpTarget {
    Resolved {
        target: Shared<MaybeUnresolvedControlFlowBlock>,
    },
    Unresolved {
        label: InternedString,
    },
}
