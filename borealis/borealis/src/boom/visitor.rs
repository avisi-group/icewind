//! Visitor pattern for BOOM AST
//!
//! Visitor trait has overridable methods

use {
    crate::boom::{
        Definition, Expression, FunctionDefinition, FunctionSignature, Literal, NamedType,
        NamedValue, Operation, Parameter, Statement, Type, Value, control_flow::ControlFlowBlock,
    },
    sailrs::shared::Shared,
};

/// Visitor trait for interacting with the BOOM AST
#[allow(missing_docs)]
pub trait Visitor: Sized {
    fn visit_definition(&mut self, node: &Definition) {
        node.walk(self);
    }

    fn visit_function_definition(&mut self, node: &FunctionDefinition) {
        node.walk(self);
    }

    fn visit_function_signature(&mut self, node: &FunctionSignature) {
        node.walk(self);
    }

    /// WARNING: does not correctly handle cycles, use
    /// `visit_function_definition`!
    fn visit_control_flow_block(&mut self, block: &ControlFlowBlock) {
        block.walk(self);
    }

    fn visit_named_type(&mut self, node: &NamedType) {
        node.walk(self);
    }

    fn visit_named_value(&mut self, node: &NamedValue) {
        node.walk(self);
    }

    fn visit_type(&mut self, node: Shared<Type>) {
        node.walk(self);
    }

    fn visit_parameter(&mut self, node: &Parameter) {
        node.walk(self);
    }

    fn visit_statement(&mut self, node: Shared<Statement>) {
        node.get().walk(self);
    }

    fn visit_expression(&mut self, node: &Expression) {
        node.walk(self);
    }

    fn visit_value(&mut self, node: Shared<Value>) {
        node.get().walk(self);
    }

    fn visit_literal(&mut self, node: Shared<Literal>) {
        node.get().walk(self);
    }

    fn visit_operation(&mut self, node: &Operation) {
        node.walk(self);
    }
}

/// Trait encapsulating the traversing logic for the AST
pub trait Walkable {
    /// Visit children of the current node
    fn walk<V: Visitor>(&self, visitor: &mut V);
}
