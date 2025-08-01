use {
    crate::rudder::opt::OptimizationContext,
    common::{
        arena::{Arena, Ref},
        rudder::{block::Block, constant::Constant, function::Function, statement::Statement},
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
        Statement::BitExtract {
            value,
            start,
            width,
        } => {
            bit_extract_entire_value(arena, stmt, value, start, width)
                || bit_extract_of_bit_insert(arena, stmt, value, start, width)
        }

        _ => {
            //trace!("candidate for folding not implemented: {}", stmt);
            false
        }
    }
}

// bit extract of the entire value should be replaced with the value
fn bit_extract_entire_value(
    arena: &mut Arena<Statement>,
    stmt: Ref<Statement>,
    value: Ref<Statement>,
    start: Ref<Statement>,
    width: Ref<Statement>,
) -> bool {
    let Statement::Constant(Constant::SignedInteger { value: 0, .. }) = start.get(arena) else {
        return false;
    };

    let Statement::Constant(Constant::SignedInteger { value: width, .. }) = width.get(arena) else {
        return false;
    };

    let value = value.get(arena).clone();

    if *width != i64::try_from(value.typ(arena).unwrap().width_bits()).unwrap() {
        return false;
    }

    stmt.get_mut(arena).replace(value);

    true
}

// bit extract of an equivalent bit insert
fn bit_extract_of_bit_insert(
    arena: &mut Arena<Statement>,
    stmt: Ref<Statement>,
    value: Ref<Statement>,
    start: Ref<Statement>,
    width: Ref<Statement>,
) -> bool {
    let Statement::Constant(Constant::SignedInteger {
        value: extract_start,
        ..
    }) = start.get(arena)
    else {
        return false;
    };

    let Statement::Constant(Constant::SignedInteger {
        value: extract_width,
        ..
    }) = width.get(arena)
    else {
        return false;
    };

    let Statement::BitInsert {
        target: _,
        source: insert_source,
        start: insert_start,
        width: insert_width,
    } = value.get(arena)
    else {
        return false;
    };

    let Statement::Constant(Constant::SignedInteger {
        value: insert_start,
        ..
    }) = insert_start.get(arena)
    else {
        return false;
    };

    let Statement::Constant(Constant::SignedInteger {
        value: insert_width,
        ..
    }) = insert_width.get(arena)
    else {
        return false;
    };

    if !(insert_start == extract_start && insert_width == extract_width) {
        return false;
    }

    let insert_source = insert_source.get(arena).clone();
    stmt.get_mut(arena).replace(insert_source);

    true
}
