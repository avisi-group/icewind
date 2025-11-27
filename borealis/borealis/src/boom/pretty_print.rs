//! BOOM AST pretty printing

use {
    crate::boom::{
        Ast, Definition, Expression, FunctionDefinition, FunctionSignature, Literal, NamedType,
        NamedValue, Operation, Parameter, Size, Statement, Type, Value,
        control_flow::{ControlFlowBlock, Terminator},
        visitor::Visitor,
    },
    common::intern::InternedString,
    itertools::Itertools,
    sailrs::shared::Shared,
    std::{
        io::Write,
        rc::Rc,
        sync::atomic::{AtomicUsize, Ordering},
    },
};

const PADDING: &str = "  ";

/// Pretty-print BOOM AST
pub fn print_ast<W: Write>(w: &mut W, ast: Shared<Ast>) {
    let Ast {
        registers,
        functions,
        constants,
        enums,
        structs,
        unions,
        pragmas,
    } = &*ast.get();

    let mut visitor = PrettyPrinter::new(w);

    pragmas
        .iter()
        .sorted_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()))
        .for_each(|(key, value)| {
            visitor.prindent(format!("#{key} {value}"));
        });

    constants
        .iter()
        .sorted_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()))
        .for_each(|(name, value)| {
            writeln!(visitor.writer, "constant {name}: {value}").unwrap();
        });
    writeln!(visitor.writer).unwrap();

    enums
        .iter()
        .sorted_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()))
        .for_each(|(name, variants)| {
            writeln!(visitor.writer, "enum {name} {{").unwrap();
            for (i, variant) in variants.iter().enumerate() {
                writeln!(visitor.writer, "\t{variant} = {i},").unwrap();
            }
            writeln!(visitor.writer, "}}").unwrap();
        });

    structs
        .iter()
        .sorted_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()))
        .for_each(|(name, fields)| {
            visitor.prindentln(format!("struct {name} {{"));

            {
                let _h = visitor.indent();
                fields.iter().for_each(|NamedType { name, typ }| {
                    visitor.prindent(format!("{name}: "));
                    visitor.visit_type(typ.clone());
                    writeln!(visitor.writer, ",").unwrap();
                });
            }

            visitor.prindentln("}");
        });

    unions
        .iter()
        .sorted_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()))
        .for_each(|(name, fields)| {
            visitor.prindentln(format!("union {name} {{"));

            {
                let _h = visitor.indent();
                fields.iter().for_each(|NamedType { name, typ }| {
                    visitor.prindent(format!("{name}: "));
                    visitor.visit_type(typ.clone());
                    writeln!(visitor.writer, ",").unwrap();
                });
            }

            visitor.prindentln("}");
        });

    registers
        .iter()
        .sorted_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()))
        .for_each(|(name, typ)| {
            write!(visitor.writer, "register {name}: ").unwrap();
            visitor.visit_type(typ.clone());
            writeln!(visitor.writer).unwrap();
        });

    functions
        .iter()
        .sorted_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()))
        .for_each(|(_, fundef)| visitor.visit_function_definition(fundef));
}

/// Pretty-print BOOM statement
pub fn print_statement<W: Write>(w: &mut W, statement: Shared<Statement>) {
    let mut visitor = PrettyPrinter::new(w);
    visitor.visit_statement(statement);
}

pub fn print_value<W: Write>(w: &mut W, value: Shared<Value>) {
    let mut visitor = PrettyPrinter::new(w);
    visitor.visit_value(value);
}

/// Pretty-print BOOM AST
pub struct PrettyPrinter<'writer, W> {
    indent: Rc<AtomicUsize>,
    writer: &'writer mut W,
}

impl<'writer, W: Write> PrettyPrinter<'writer, W> {
    /// Creates a new `BoomPrettyPrinter` with the supplied writer
    pub fn new(writer: &'writer mut W) -> Self {
        Self {
            indent: Rc::new(AtomicUsize::new(0)),
            writer,
        }
    }
}

impl<'writer, W: Write> PrettyPrinter<'writer, W> {
    fn prindent<T: AsRef<str>>(&mut self, s: T) {
        write!(
            self.writer,
            "{}{}",
            PADDING.repeat(self.indent.load(Ordering::SeqCst)),
            s.as_ref()
        )
        .unwrap();
    }

    fn prindentln<T: AsRef<str>>(&mut self, s: T) {
        self.prindent(s);
        writeln!(self.writer).unwrap();
    }

    fn indent(&self) -> IndentHandle {
        self.indent.fetch_add(1, Ordering::SeqCst);
        IndentHandle {
            indent: self.indent.clone(),
        }
    }

    fn print_control_flow_graph(&mut self, entry_block: ControlFlowBlock) {
        entry_block.iter().for_each(|b| {
            writeln!(self.writer, "    {}:", b).unwrap();
            {
                b.statements().iter().for_each(|stmt| {
                    write!(self.writer, "        ").unwrap();
                    self.visit_statement(stmt.clone());
                    writeln!(self.writer).unwrap();
                });

                match b.terminator() {
                    Terminator::Return(value) => {
                        write!(self.writer, "        return ").unwrap();
                        if let Some(value) = value {
                            self.visit_value(Shared::new(value));
                        }
                        writeln!(self.writer, ";").unwrap();
                    }
                    Terminator::Conditional {
                        condition,
                        target,
                        fallthrough,
                    } => {
                        write!(self.writer, "        if (").unwrap();
                        self.visit_value(Shared::new(condition));
                        writeln!(self.writer, ") {{").unwrap();
                        writeln!(self.writer, "            goto {target};").unwrap();
                        writeln!(self.writer, "        }} else {{").unwrap();
                        writeln!(self.writer, "            goto {fallthrough};").unwrap();
                        writeln!(self.writer, "        }}").unwrap();
                    }
                    Terminator::Unconditional { target } => {
                        writeln!(self.writer, "        goto {target};").unwrap();
                    }
                    Terminator::Panic(value) => {
                        write!(self.writer, "        panic ").unwrap();
                        self.visit_value(Shared::new(value));
                        writeln!(self.writer, ";").unwrap();
                    }
                }
            }
            writeln!(self.writer).unwrap();
        });
    }
}

struct IndentHandle {
    indent: Rc<AtomicUsize>,
}

impl Drop for IndentHandle {
    fn drop(&mut self) {
        self.indent.fetch_sub(1, Ordering::SeqCst);
    }
}

impl<'writer, W: Write> Visitor for PrettyPrinter<'writer, W> {
    fn visit_definition(&mut self, node: &Definition) {
        match node {
            Definition::Struct { name, fields } => {
                self.prindentln(format!("struct {name} {{"));

                {
                    let _h = self.indent();
                    fields.iter().for_each(|NamedType { name, typ }| {
                        self.prindent(format!("{name}: "));
                        self.visit_type(typ.clone());
                        writeln!(self.writer, ",").unwrap();
                    });
                }

                self.prindentln("}");
            }
            Definition::Pragma { key, value } => {
                self.prindent(format!("#{key} {value}"));
            }
            Definition::Union { name, fields } => {
                self.prindentln(format!("union {name} {{"));

                {
                    let _h = self.indent();
                    fields.iter().for_each(|NamedType { name, typ }| {
                        self.prindent(format!("{name}: "));
                        self.visit_type(typ.clone());
                        writeln!(self.writer, ",").unwrap();
                    });
                }

                self.prindentln("}");
            }
        }
    }

    fn visit_function_definition(&mut self, node: &FunctionDefinition) {
        self.visit_function_signature(&node.signature);

        self.print_control_flow_graph(node.entry_block.clone());

        writeln!(self.writer, "}}").unwrap();
    }

    fn visit_function_signature(
        &mut self,
        FunctionSignature {
            name,
            parameters,
            return_type,
        }: &FunctionSignature,
    ) {
        self.prindent(format!("fn {}(", name));

        let parameters = parameters.get();
        let mut parameters = parameters.iter();
        if let Some(param) = parameters.next() {
            self.visit_parameter(param);
        }
        for param in parameters {
            write!(self.writer, ", ").unwrap();
            self.visit_parameter(param);
        }

        write!(self.writer, ") -> ").unwrap();
        if let Some(ret) = return_type {
            self.visit_type(ret.clone());
        } else {
            write!(self.writer, "void").unwrap();
        }

        writeln!(self.writer, " {{").unwrap();
    }

    fn visit_statement(&mut self, node: Shared<Statement>) {
        match &*node.get() {
            Statement::VariableDeclaration { name, typ } => {
                self.visit_type(typ.clone());
                write!(self.writer, " {name};").unwrap();
            }

            Statement::Copy { expression, value } => {
                self.visit_expression(expression);
                write!(self.writer, " = ").unwrap();
                self.visit_value(value.clone());
                write!(self.writer, ";").unwrap();
            }

            Statement::FunctionCall {
                expression,
                name,
                arguments,
            } => {
                if let Some(expression) = expression {
                    self.visit_expression(expression);
                    write!(self.writer, " = ").unwrap();
                }

                write!(self.writer, "{name}(").unwrap();

                let mut args = arguments.iter();
                if let Some(arg) = args.next() {
                    self.visit_value(arg.clone());
                }
                args.for_each(|arg| {
                    write!(self.writer, ", ").unwrap();
                    self.visit_value(arg.clone());
                });

                write!(self.writer, ");").unwrap();
            }

            Statement::Comment(str) => write!(self.writer, "// {str}").unwrap(),
            Statement::Label(str) => write!(self.writer, "// label({str})").unwrap(),
            Statement::Exit(str) => write!(self.writer, "// exit({str})").unwrap(),

            Statement::If { .. } | Statement::Jump { .. } | Statement::Goto(_) => {
                panic!(
                    "control flow statements should have been removed by this point: {:?}",
                    node
                )
            }

            Statement::End(_) => todo!(),
            Statement::Undefined => todo!(),
            Statement::Panic(value) => {
                write!(self.writer, "panic(").unwrap();
                self.visit_value(value.clone());
                write!(self.writer, ")").unwrap();
            }
        }
    }

    fn visit_parameter(&mut self, node: &Parameter) {
        self.visit_type(node.typ.clone());

        write!(self.writer, " {}", node.name).unwrap();
    }

    fn visit_type(&mut self, node: Shared<Type>) {
        match &*node.get() {
            Type::Unit => write!(self.writer, "()"),
            Type::String => write!(self.writer, "str"),
            Type::Bool => write!(self.writer, "bool"),
            Type::Bit => write!(self.writer, "bit"),
            Type::Real => write!(self.writer, "real"),
            Type::Float => write!(self.writer, "float"),
            Type::RoundingMode => write!(self.writer, "roundingmode"),

            Type::Integer { size } => {
                write!(self.writer, "i").unwrap();

                match size {
                    Size::Static(size) => write!(self.writer, "{size}").unwrap(),

                    Size::Unknown => (),
                };

                Ok(())
            }

            Type::Bits { size } => {
                write!(self.writer, "bv").unwrap();
                match size {
                    Size::Static(size) => write!(self.writer, "{size}").unwrap(),
                    Size::Unknown => (),
                };

                Ok(())
            }
            Type::Constant(i) => write!(self.writer, "constant({i})"),
            Type::Struct { name, .. } | Type::Union { name, .. } => {
                write!(self.writer, "{name}")
            }
            Type::Vector { element_type } => {
                write!(self.writer, "vec<").unwrap();
                self.visit_type(element_type.clone());
                write!(self.writer, ">")
            }
            Type::FixedVector {
                length,
                element_type,
            } => {
                write!(self.writer, "[").unwrap();
                self.visit_type(element_type.clone());
                write!(self.writer, "; {length}]")
            }
            Type::Reference(typ) => {
                write!(self.writer, "&").unwrap();
                self.visit_type(typ.clone());
                Ok(())
            }
            Type::Tuple(ts) => {
                write!(self.writer, "(").unwrap();
                ts.iter().for_each(|t| {
                    self.visit_type(t.clone());
                    write!(self.writer, ", ").unwrap();
                });
                write!(self.writer, ")")
            }
        }
        .unwrap()
    }

    fn visit_value(&mut self, node: Shared<Value>) {
        match &*node.get() {
            Value::Identifier(ident) => write!(self.writer, "{ident}").unwrap(),
            Value::Literal(literal) => self.visit_literal(literal.clone()),
            Value::Operation(op) => self.visit_operation(op),
            Value::Struct { name, fields } => {
                write!(self.writer, "struct {name} {{").unwrap();

                for NamedValue { name, value } in fields {
                    write!(self.writer, "{name}: ").unwrap();
                    self.visit_value(value.clone());
                    write!(self.writer, ",").unwrap();
                }

                write!(self.writer, "}}").unwrap();
            }
            Value::Field { value, field_name } => {
                self.visit_value(value.clone());
                write!(self.writer, ".{field_name}").unwrap();
            }
            Value::CtorKind { value, identifier } => {
                self.visit_value(value.clone());
                write!(self.writer, " is {identifier}").unwrap();
            }
            Value::CtorUnwrap { value, identifier } => {
                self.visit_value(value.clone());
                write!(self.writer, " as {identifier}").unwrap();
            }
            Value::Real {
                numerator,
                denominator,
            } => {
                write!(self.writer, "Real(").unwrap();

                self.visit_value(numerator.clone());
                write!(self.writer, ", ").unwrap();
                self.visit_value(denominator.clone());

                write!(self.writer, ")").unwrap();
            }
            Value::Tuple(values) => {
                write!(self.writer, "(").unwrap();
                values.iter().for_each(|v| {
                    self.visit_value(v.clone());
                    write!(self.writer, ", ").unwrap();
                });

                write!(self.writer, ")").unwrap();
            }
            Value::VectorAccess { value, index } => {
                self.visit_value(value.clone());
                write!(self.writer, "[").unwrap();
                self.visit_value(index.clone());
                write!(self.writer, "]").unwrap();
            }
            Value::VectorMutate {
                vector,
                element,
                index,
            } => {
                write!(self.writer, "(").unwrap();
                self.visit_value(vector.clone());
                write!(self.writer, "[").unwrap();
                self.visit_value(index.clone());
                write!(self.writer, "] = ").unwrap();
                self.visit_value(element.clone());
                write!(self.writer, ")").unwrap();
            }
        }
    }

    fn visit_control_flow_block(&mut self, _: &ControlFlowBlock) {
        unreachable!()
    }

    fn visit_named_type(&mut self, NamedType { name, typ }: &NamedType) {
        write!(self.writer, "{name}: ").unwrap();
        self.visit_type(typ.clone());
        write!(self.writer, ", ").unwrap();
    }

    fn visit_named_value(&mut self, NamedValue { name, value }: &NamedValue) {
        write!(self.writer, "{name}: ").unwrap();
        self.visit_value(value.clone());
        write!(self.writer, ", ").unwrap();
    }

    fn visit_expression(&mut self, node: &Expression) {
        match node {
            Expression::Identifier(ident) => write!(self.writer, "{ident}").unwrap(),
            Expression::Field { expression, field } => {
                self.visit_expression(expression);
                write!(self.writer, ".{field}").unwrap();
            }
            Expression::Address(expression) => {
                write!(self.writer, "&").unwrap();
                self.visit_expression(expression);
            }
            Expression::Tuple(exprs) => {
                write!(self.writer, "(").unwrap();
                exprs.iter().for_each(|e| {
                    self.visit_expression(e);
                    write!(self.writer, ", ").unwrap();
                });
                write!(self.writer, ")").unwrap();
            }
        }
    }

    fn visit_literal(&mut self, node: Shared<Literal>) {
        match &*node.get() {
            Literal::Int(bi) => write!(self.writer, "{bi}"),
            Literal::Bits(bits) => write!(self.writer, "{:x?}", bits),
            Literal::Bit(bit) => write!(self.writer, "{}", bit.value()),
            Literal::Bool(bool) => write!(self.writer, "{bool}"),
            Literal::String(s) => write!(self.writer, "{s:?}"),
            Literal::Unit => write!(self.writer, "()"),
            Literal::Reference(s) => write!(self.writer, "&{s}"),
            Literal::Undefined => write!(self.writer, "undefined"),
            Literal::Vector(vec) => {
                write!(self.writer, "[").unwrap();
                for element in vec {
                    self.visit_literal(element.clone());
                }
                write!(self.writer, "]")
            }
        }
        .unwrap()
    }

    fn visit_operation(&mut self, node: &Operation) {
        fn emit_op2<W: Write>(
            printer: &mut PrettyPrinter<'_, W>,
            lhs: &Shared<Value>,
            rhs: &Shared<Value>,
            op: &str,
        ) {
            write!(printer.writer, "(").unwrap();
            printer.visit_value(lhs.clone());
            write!(printer.writer, " {op} ").unwrap();
            printer.visit_value(rhs.clone());
            write!(printer.writer, ")").unwrap();
        }

        match node {
            Operation::Not(value) => {
                write!(self.writer, "!").unwrap();
                self.visit_value(value.clone());
            }
            Operation::Complement(value) => {
                write!(self.writer, "~").unwrap();
                self.visit_value(value.clone());
            }
            Operation::Equal(lhs, rhs) => emit_op2(self, lhs, rhs, "=="),
            Operation::NotEqual(lhs, rhs) => emit_op2(self, lhs, rhs, "!="),
            Operation::LessThan(lhs, rhs) => emit_op2(self, lhs, rhs, "<"),
            Operation::GreaterThan(lhs, rhs) => emit_op2(self, lhs, rhs, ">"),
            Operation::LessThanOrEqual(lhs, rhs) => emit_op2(self, lhs, rhs, "<="),
            Operation::GreaterThanOrEqual(lhs, rhs) => emit_op2(self, lhs, rhs, ">="),
            Operation::Subtract(lhs, rhs) => emit_op2(self, lhs, rhs, "-"),
            Operation::Add(lhs, rhs) => emit_op2(self, lhs, rhs, "+"),
            Operation::Multiply(lhs, rhs) => emit_op2(self, lhs, rhs, "*"),
            Operation::Or(lhs, rhs) => emit_op2(self, lhs, rhs, "|"),
            Operation::Xor(lhs, rhs) => emit_op2(self, lhs, rhs, "^"),
            Operation::And(lhs, rhs) => emit_op2(self, lhs, rhs, "&"),
            Operation::Divide(lhs, rhs) => emit_op2(self, lhs, rhs, "/"),
            Operation::LeftShift(lhs, rhs) => emit_op2(self, lhs, rhs, "<<"),
            Operation::RightShift(lhs, rhs) => emit_op2(self, lhs, rhs, ">>"),

            Operation::RotateLeft(lhs, rhs) => emit_op2(self, lhs, rhs, "<<<"),
            Operation::RotateRight(lhs, rhs) => emit_op2(self, lhs, rhs, ">>>"),

            Operation::Cast(value, typ) => {
                self.visit_value(value.clone());
                write!(self.writer, " as ").unwrap();
                self.visit_type(typ.clone());
            }
        }
    }
}
