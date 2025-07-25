use {
    crate::rudder::opt::OptimizationContext,
    common::{
        arena::{Arena, Ref},
        rudder::{
            block::Block,
            constant::Constant,
            function::Function,
            statement::{BinaryOperationKind, Location, Statement, build_at, cast_at},
            types::Type,
        },
    },
};

pub fn run(_ctx: &OptimizationContext, f: &mut Function) -> bool {
    let mut changed = false;
    for block in f.block_iter().collect::<Vec<_>>().into_iter() {
        changed |= run_on_block(f.arena_mut(), block);
    }

    changed
}

/// Replace vector access on registers and locals with adding to the indices and
/// offset respectively
fn run_on_block(arena: &mut Arena<Block>, block: Ref<Block>) -> bool {
    let mut did_change = false;

    for stmt in block
        .get(arena)
        .statements()
        .iter()
        .copied()
        .collect::<Vec<_>>()
    {
        // if we have a write reg of an assign element of a read reg
        // replace with single write reg to element
        if let Statement::WriteRegister {
            offset: write_offset,
            value: write_value,
        } = stmt.get(block.get(arena).arena()).clone()
        {
            if let Statement::AssignElement {
                vector: assign_vector,
                value: assign_value,
                index: assign_index,
            } = write_value.get(block.get(arena).arena()).clone()
            {
                if let Statement::ReadRegister {
                    typ: _read_type,
                    offset: _read_offset,
                } = assign_vector.get(block.get(arena).arena()).clone()
                {
                    // write-register
                    // offset = write_offset + index * element type width bytes
                    // value = assign_value

                    //assert_eq!(write_offset.kind(), read_offset.kind());

                    let vector_width = build_at(
                        block,
                        arena,
                        Statement::Constant(Constant::new_unsigned(
                            assign_value
                                .get(block.get(arena).arena())
                                .typ(block.get(arena).arena())
                                .unwrap()
                                .width_bytes()
                                .try_into()
                                .unwrap(),
                            16,
                        )),
                        Location::Before(stmt),
                    );
                    let vector_offset = build_at(
                        block,
                        arena,
                        Statement::BinaryOperation {
                            kind: BinaryOperationKind::Multiply,
                            lhs: assign_index,
                            rhs: vector_width,
                        },
                        Location::Before(stmt),
                    );
                    let offset = build_at(
                        block,
                        arena,
                        Statement::BinaryOperation {
                            kind: BinaryOperationKind::Add,
                            lhs: write_offset,
                            rhs: vector_offset,
                        },
                        Location::Before(stmt),
                    );

                    // after inserting offset calculation, re-insert all the post-statements

                    // replace kind to make sure future uses aren't invalidated
                    // todo: actually this is a write, we can just delete it and build it again
                    stmt.get_mut(block.get_mut(arena).arena_mut()).replace(
                        Statement::WriteRegister {
                            offset,
                            value: assign_value,
                        },
                    );

                    did_change = true;
                }
            }
        }

        // if we're reading an element of a vec
        // see if index is constant (check if the bundle is constant)
        // if vector is a register read, add index to offset
        // todo: if vector is a local variable read, add index to indices
        if let Statement::ReadElement { vector, index } = stmt.get(block.get(arena).arena()).clone()
        {
            if let Statement::ReadRegister { offset, .. } =
                vector.get(block.get(arena).arena()).clone()
            {
                let element_type = stmt
                    .get(block.get(arena).arena())
                    .typ(block.get(arena).arena())
                    .unwrap();

                let index = cast_at(block, arena, index, Type::s64(), Location::Before(stmt));

                let offset = cast_at(block, arena, offset, Type::s64(), Location::Before(stmt));

                let typ_width = build_at(
                    block,
                    arena,
                    Statement::Constant(Constant::new_signed(
                        i64::try_from(element_type.width_bytes()).unwrap(),
                        64,
                    )),
                    Location::Before(stmt),
                );

                let index_scaled = build_at(
                    block,
                    arena,
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::Multiply,
                        lhs: index,
                        rhs: typ_width,
                    },
                    Location::Before(stmt),
                );

                let new_offset = build_at(
                    block,
                    arena,
                    Statement::BinaryOperation {
                        kind: BinaryOperationKind::Add,
                        lhs: index_scaled,
                        rhs: offset,
                    },
                    Location::Before(stmt),
                );

                // after inserting offset calculation, re-insert all the post-statements

                // replace kind to make sure future uses aren't invalidated
                // todo: actually this is a write, we can just delete it and build it again
                stmt.get_mut(block.get_mut(arena).arena_mut())
                    .replace(Statement::ReadRegister {
                        typ: element_type,
                        offset: new_offset,
                    });

                did_change = true;
            }
        }
    }

    did_change
}
