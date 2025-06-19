//! Inlines calls to functions with only a single block

use {
    crate::rudder::opt::{INTRINSICS, OptimizationContext},
    common::{
        arena::{Arena, Ref},
        hashmap::HashMap,
        id::Id,
        rudder::{
            Model,
            block::Block,
            function::{Function, Symbol},
            statement::{Location, Statement, build_at},
            types::{PrimitiveType, Type},
        },
    },
    rayon::prelude::*,
};

pub fn run(_ctx: &OptimizationContext, model: &mut Model) -> bool {
    let mut changed = false;

    let single_block_functions = model
        .functions()
        .par_iter()
        .filter(|(name, _)| !INTRINSICS.contains(&name.as_ref()))
        .filter(|(_, f)| f.block_iter().count() == 1)
        .map(|(name, f)| {
            //let block = f.entry_block().get(f.arena());
            (*name, f.clone())
        })
        .collect::<HashMap<_, _>>();

    for (_, function) in model
        .functions_mut()
        .iter_mut()
        .filter(|(name, _)| !INTRINSICS.contains(&name.as_ref()))
    {
        for b in function.block_iter().collect::<Vec<_>>().into_iter() {
            let mut insert = None;

            {
                let Block {
                    statement_arena,
                    statements,
                } = b.get_mut(function.arena_mut());

                for (index, s) in statements.iter().enumerate() {
                    let statement = s.get(statement_arena);

                    if let Statement::Call { target, .. } = statement {
                        if let Some(func) = single_block_functions.get(target) {
                            insert = Some((index, func));
                            break;
                        }
                    }
                }
            }

            if let Some((call_index, source_function)) = insert {
                inline_function(function, b, call_index, source_function);

                changed = true;
            }
        }
    }

    changed
}

fn inline_function(
    function: &mut Function,
    b: Ref<Block>,
    call_index: usize,
    source_function: &Function,
) {
    log::debug!(
        "call to {:?} at {call_index} in {:?} being inlined",
        source_function.name(),
        function.name()
    );

    let source_block = source_function.entry_block().get(source_function.arena());
    let return_type = source_function.return_type();

    // get the statement being replaced
    let call_statement = b.get(function.arena()).statements()[call_index];

    // pull out arguments
    let Statement::Call { args, .. } = call_statement.get(b.get(function.arena()).arena()).clone()
    else {
        panic!()
    };

    let mut symbol_map = HashMap::default();

    // generate symbol for return value
    let intermediate_return_symbol = Symbol::new(
        format!("intermediate_return_{}", Id::new()).into(),
        return_type.unwrap_or(Type::Primitive(PrimitiveType::UnsignedInteger(0))),
    );
    symbol_map.insert(
        intermediate_return_symbol.clone(),
        intermediate_return_symbol.clone(),
    );
    function.add_local_variable(intermediate_return_symbol.clone());

    // import all the local variables from the source block
    for source_local_var in source_function.local_variables() {
        let mangled = mangle_symbol(&source_local_var);
        symbol_map.insert(source_local_var, mangled.clone());
        function.add_local_variable(mangled);
    }

    // create symbols for function parameters, and write the
    for (i, param) in source_function.parameters().into_iter().enumerate() {
        let mangled = mangle_symbol(&param);
        symbol_map.insert(param, mangled.clone());

        function.add_local_variable(mangled.clone());

        let value = args[i].clone();

        build_at(
            b,
            function.arena_mut(),
            Statement::WriteVariable {
                symbol: mangled,
                value,
            },
            Location::Before(call_statement),
        );
    }

    // make our own local copy to modify
    let mut source_block = source_block.clone();

    // replace final return with a write variable
    let mut source_statements = source_block.statements().to_owned();
    let final_statement = source_statements
        .last()
        .unwrap()
        .get_mut(source_block.arena_mut());
    if let Statement::Return { value } = final_statement {
        if let Some(value) = value {
            *final_statement = Statement::WriteVariable {
                symbol: intermediate_return_symbol.clone(),
                value: *value,
            };
        } else {
            // remove
            source_statements.pop().unwrap();
        }
    } else {
        // probably a panic, leave in
    };

    let mut source_arena = Arena::new();
    let source_block = source_arena.insert(source_block);

    // map from src to dest refs
    let mut statement_map = HashMap::default();

    for source_statement in source_statements.into_iter() {
        import_statement_at(
            source_block,
            b,
            &mut source_arena,
            function.arena_mut(),
            source_statement,
            &mut statement_map,
            &mut symbol_map,
            common::rudder::statement::Location::Before(call_statement),
        );
    }

    // replace call with read_variable of intermediate_return_symbol, so uses of the
    // call work
    call_statement
        .get_mut(b.get_mut(function.arena_mut()).arena_mut())
        .replace_kind(Statement::ReadVariable {
            symbol: intermediate_return_symbol,
        });
}

fn mangle_symbol(symbol: &Symbol) -> Symbol {
    Symbol::new(
        format!("mangled_local_var_{}_{}", Id::new(), symbol.name()).into(),
        symbol.typ(),
    )
}

pub fn import_statement_at(
    source_block: Ref<Block>,
    target_block: Ref<Block>,
    source_arena: &mut Arena<Block>,
    target_arena: &mut Arena<Block>,
    source_statement: Ref<Statement>,
    statement_mapping: &mut HashMap<Ref<Statement>, Ref<Statement>>,
    symbol_mapping: &HashMap<Symbol, Symbol>,
    location: Location,
) -> Ref<Statement> {
    let mapped_kind = match source_statement
        .get(source_block.get(&source_arena).arena())
        .clone()
    {
        Statement::BinaryOperation { kind, lhs, rhs } => Statement::BinaryOperation {
            kind,
            lhs: statement_mapping.get(&lhs).unwrap().clone(),
            rhs: statement_mapping.get(&rhs).unwrap().clone(),
        },
        Statement::TernaryOperation { kind, a, b, c } => Statement::TernaryOperation {
            kind,
            a: statement_mapping.get(&a).unwrap().clone(),
            b: statement_mapping.get(&b).unwrap().clone(),
            c: statement_mapping.get(&c).unwrap().clone(),
        },
        Statement::Constant(c) => Statement::Constant(c),
        Statement::ReadVariable { symbol } => Statement::ReadVariable {
            symbol: symbol_mapping.get(&symbol).unwrap().clone(),
        },
        Statement::WriteVariable { symbol, value } => Statement::WriteVariable {
            symbol: symbol_mapping
                .get(&symbol)
                .unwrap_or_else(|| panic!("failed to find symbol mapping for {symbol:?} in statment {source_statement:?} in source block {source_block:?}"))
                .clone(),
            value: statement_mapping.get(&value).unwrap().clone(),
        },
        Statement::ReadRegister { typ, offset } => Statement::ReadRegister {
            typ,
            offset: statement_mapping.get(&offset).unwrap().clone(),
        },
        Statement::WriteRegister { offset, value } => Statement::WriteRegister {
            offset: statement_mapping.get(&offset).unwrap().clone(),
            value: statement_mapping.get(&value).unwrap().clone(),
        },
        Statement::ReadMemory {
            address: offset,
            size,
        } => Statement::ReadMemory {
            address: statement_mapping.get(&offset).unwrap().clone(),
            size: statement_mapping.get(&size).unwrap().clone(),
        },
        Statement::WriteMemory {
            address: offset,
            value,
        } => Statement::WriteMemory {
            address: statement_mapping.get(&offset).unwrap().clone(),
            value: statement_mapping.get(&value).unwrap().clone(),
        },
        Statement::ReadPc => Statement::ReadPc,
        Statement::WritePc { value } => Statement::WritePc {
            value: statement_mapping.get(&value).unwrap().clone(),
        },
        Statement::UnaryOperation { kind, value } => Statement::UnaryOperation {
            kind,
            value: statement_mapping.get(&value).unwrap().clone(),
        },
        Statement::ShiftOperation {
            kind,
            value,
            amount,
        } => Statement::ShiftOperation {
            kind,
            value: statement_mapping.get(&value).unwrap().clone(),
            amount: statement_mapping.get(&amount).unwrap().clone(),
        },
        Statement::Call {
            target,
            args,
            return_type,
        } => {
            let args = args
                .iter()
                .map(|stmt| statement_mapping.get(stmt).unwrap_or_else(|| panic!("could not get mapping for source statement argument {stmt:?} in call statement {source_statement:?} in source block {source_block:?}")).clone())
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
            value: statement_mapping
                .get(&value)
                .unwrap_or_else(|| {
                    panic!(
                        "{statement_mapping:?}, {:?}",
                        source_statement
                            .get(source_block.get(&source_arena).arena())
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
            value: statement_mapping.get(&value).unwrap().clone(),
            width: statement_mapping.get(&length).unwrap().clone(),
        },
        Statement::Jump { target } => Statement::Jump { target },
        Statement::Branch {
            condition,
            true_target,
            false_target,
        } => Statement::Branch {
            condition: statement_mapping.get(&condition).unwrap().clone(),
            true_target,
            false_target,
        },
        Statement::PhiNode { .. } => todo!(),
        Statement::Return { value } => Statement::Return {
            value: value.map(|value| statement_mapping.get(&value).unwrap().clone()),
        },
        Statement::Select {
            condition,
            true_value,
            false_value,
        } => Statement::Select {
            condition: statement_mapping.get(&condition).unwrap().clone(),
            true_value: statement_mapping.get(&true_value).unwrap().clone(),
            false_value: statement_mapping.get(&false_value).unwrap().clone(),
        },
        Statement::BitExtract {
            value,
            start,
            width: length,
        } => Statement::BitExtract {
            value: statement_mapping.get(&value).unwrap().clone(),
            start: statement_mapping.get(&start).unwrap().clone(),
            width: statement_mapping.get(&length).unwrap().clone(),
        },
        Statement::BitInsert {
            target,
            source,
            start,
            width: length,
        } => Statement::BitInsert {
            target: statement_mapping.get(&target).unwrap().clone(),
            source: statement_mapping.get(&source).unwrap().clone(),
            start: statement_mapping.get(&start).unwrap().clone(),
            width: statement_mapping.get(&length).unwrap().clone(),
        },
          Statement::BitReplicate {
            pattern,
            count,

        } => Statement::BitReplicate {
            pattern: statement_mapping.get(&pattern).unwrap().clone(),
            count: statement_mapping.get(&count).unwrap().clone(),

        },
        Statement::ReadElement { vector, index } => Statement::ReadElement {
            vector: statement_mapping.get(&vector).unwrap().clone(),
            index: statement_mapping.get(&index).unwrap().clone(),
        },
        Statement::AssignElement {
            vector,
            value,
            index,
        } => Statement::AssignElement {
            vector: statement_mapping.get(&vector).unwrap().clone(),
            value: statement_mapping.get(&value).unwrap().clone(),
            index: statement_mapping.get(&index).unwrap().clone(),
        },
        Statement::Panic(stmt) => Statement::Panic(statement_mapping.get(&stmt).unwrap().clone()),

        Statement::Assert { condition } => Statement::Assert {
            condition: statement_mapping.get(&condition).unwrap().clone(),
        },

        Statement::CreateBits {
            value,
            width: length,
        } => Statement::CreateBits {
            value: statement_mapping.get(&value).unwrap().clone(),
            width: statement_mapping.get(&length).unwrap().clone(),
        },
        Statement::SizeOf { value } => Statement::SizeOf {
            value: statement_mapping.get(&value).unwrap().clone(),
        },
        Statement::MatchesUnion { value, variant } => Statement::MatchesUnion {
            value: statement_mapping.get(&value).unwrap().clone(),
            variant,
        },
        Statement::UnwrapUnion { value, variant } => Statement::UnwrapUnion {
            value: statement_mapping.get(&value).unwrap().clone(),
            variant,
        },

        Statement::TupleAccess { index, source } => Statement::TupleAccess {
            source: statement_mapping.get(&source).unwrap().clone(),
            index,
        },
        Statement::GetFlags { operation } => Statement::GetFlags {
            operation: statement_mapping.get(&operation).unwrap().clone(),
        },
        Statement::CreateTuple(values) => Statement::CreateTuple(
            values
                .iter()
                .map(|v| statement_mapping.get(v).unwrap())
                .cloned()
                .collect(),
        ),
    };

    let target_statement = build_at(target_block, target_arena, mapped_kind, location);
    statement_mapping.insert(source_statement, target_statement);
    target_statement
}
