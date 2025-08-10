use common::{
    hashmap::HashMap,
    intern::InternedString,
    rudder::{
        block::Block,
        constant::Constant,
        function::{Function, Symbol},
        statement::Statement,
        types::Type,
    },
};

pub fn example_functions() -> HashMap<InternedString, Function> {
    let mut fns = HashMap::default();
    let mut f1 = Function::new("example_f1".into(), None, vec![]);

    {
        let entry_block = f1.entry_block().get_mut(f1.arena_mut());
        let s_arena = entry_block.arena_mut();

        let _0 = s_arena.insert(Statement::Constant(Constant::new_unsigned(0, 64)));
        let _8 = s_arena.insert(Statement::Constant(Constant::new_unsigned(8, 64)));
        let _16 = s_arena.insert(Statement::Constant(Constant::new_unsigned(16, 64)));
        let _24 = s_arena.insert(Statement::Constant(Constant::new_unsigned(24, 64)));

        let r0 = s_arena.insert(Statement::ReadRegister {
            typ: Type::u64(),
            offset: _0,
        });
        let r1 = s_arena.insert(Statement::ReadRegister {
            typ: Type::u64(),
            offset: _8,
        });
        let call1 = s_arena.insert(Statement::Call {
            target: "example_f2".into(),
            args: vec![r0, r1],
            return_type: Some(Type::u64()),
        });
        let r2 = s_arena.insert(Statement::ReadRegister {
            typ: Type::u64(),
            offset: _16,
        });
        let call2 = s_arena.insert(Statement::Call {
            target: "example_f2".into(),
            args: vec![call1, r2],
            return_type: Some(Type::u64()),
        });
        let w3 = s_arena.insert(Statement::WriteRegister {
            offset: _24,
            value: call2,
        });
        let ret = s_arena.insert(Statement::Return { value: None });
        entry_block
            .set_statements([_0, _8, _16, _24, r0, r1, call1, r2, call2, w3, ret].into_iter());
    }

    let left = Symbol::new("left".into(), Type::u64());
    let right = Symbol::new("right".into(), Type::u64());
    let mut f2 = Function::new(
        "example_f2".into(),
        Some(Type::u64()),
        vec![left.clone(), right.clone()],
    );
    {
        let entry_block = f2.entry_block().get_mut(f2.arena_mut());
        let s_arena = entry_block.arena_mut();

        let left = s_arena.insert(Statement::ReadVariable { symbol: left });
        let right = s_arena.insert(Statement::ReadVariable { symbol: right });
        let add = s_arena.insert(Statement::BinaryOperation {
            kind: common::rudder::statement::BinaryOperationKind::Add,
            lhs: left,
            rhs: right,
        });
        let ret = s_arena.insert(Statement::Return { value: Some(add) });
        entry_block.set_statements([left, right, add, ret].into_iter());
    }
    fns.insert(f1.name(), f1);
    fns.insert(f2.name(), f2);

    fns
}

pub fn variable_corrupted_example(
    r0_offset: u64,
    r1_offset: u64,
    r2_offset: u64,
) -> HashMap<InternedString, Function> {
    let mut fns = HashMap::default();
    let mut func = Function::new("func_corrupted_var".into(), Some(Type::u64()), vec![]);
    let ret_val = Symbol::new("x".into(), Type::u64());
    func.add_local_variable(ret_val.clone());

    {
        let a = func.arena_mut().insert(Block::new());
        let b = func.arena_mut().insert(Block::new());
        let c = func.arena_mut().insert(Block::new());
        let d = func.arena_mut().insert(Block::new());
        let e = func.arena_mut().insert(Block::new());
        let f = func.arena_mut().insert(Block::new());
        let g = func.arena_mut().insert(Block::new());

        {
            let entry_block = func.entry_block().get_mut(func.arena_mut());
            let s_arena = entry_block.arena_mut();
            let jump = s_arena.insert(Statement::Jump { target: a });
            entry_block.set_statements([jump].into_iter());
        }

        {
            let a = a.get_mut(func.arena_mut());
            let s_arena = a.arena_mut();
            let r0_offset =
                s_arena.insert(Statement::Constant(Constant::new_unsigned(r0_offset, 64)));
            let read = s_arena.insert(Statement::ReadRegister {
                typ: Type::u64(),
                offset: r0_offset,
            });
            let branch = s_arena.insert(Statement::Branch {
                condition: read,
                true_target: b,
                false_target: c,
            });
            a.set_statements([r0_offset, read, branch].into_iter());
        }

        {
            let b = b.get_mut(func.arena_mut());
            let s_arena = b.arena_mut();
            let _5 = s_arena.insert(Statement::Constant(Constant::new_unsigned(5, 64)));
            let w = s_arena.insert(Statement::WriteVariable {
                symbol: ret_val.clone(),
                value: _5,
            });
            let jump = s_arena.insert(Statement::Jump { target: d });
            b.set_statements([_5, w, jump].into_iter());
        }

        {
            let c = c.get_mut(func.arena_mut());
            let s_arena = c.arena_mut();
            let _10 = s_arena.insert(Statement::Constant(Constant::new_unsigned(10, 64)));
            let w = s_arena.insert(Statement::WriteVariable {
                symbol: ret_val.clone(),
                value: _10,
            });
            let jump = s_arena.insert(Statement::Jump { target: d });
            c.set_statements([_10, w, jump].into_iter());
        }

        {
            let d = d.get_mut(func.arena_mut());
            let s_arena = d.arena_mut();
            let r1_offset =
                s_arena.insert(Statement::Constant(Constant::new_unsigned(r1_offset, 64)));
            let read = s_arena.insert(Statement::ReadRegister {
                typ: Type::u64(),
                offset: r1_offset,
            });
            let branch = s_arena.insert(Statement::Branch {
                condition: read,
                true_target: e,
                false_target: f,
            });
            d.set_statements([r1_offset, read, branch].into_iter());
        }

        {
            let e = e.get_mut(func.arena_mut());
            let s_arena = e.arena_mut();
            let jump = s_arena.insert(Statement::Jump { target: g });
            e.set_statements([jump].into_iter());
        }

        {
            let f = f.get_mut(func.arena_mut());
            let s_arena = f.arena_mut();
            let jump = s_arena.insert(Statement::Jump { target: g });
            f.set_statements([jump].into_iter());
        }

        {
            let g = g.get_mut(func.arena_mut());
            let s_arena = g.arena_mut();
            let read = s_arena.insert(Statement::ReadVariable {
                symbol: ret_val.clone(),
            });
            let r2_offset =
                s_arena.insert(Statement::Constant(Constant::new_unsigned(r2_offset, 64)));
            let w = s_arena.insert(Statement::WriteRegister {
                offset: r2_offset,
                value: read,
            });
            let ret = s_arena.insert(Statement::Return { value: Some(read) });
            g.set_statements([read, r2_offset, w, ret].into_iter());
        }
    }

    fns.insert(func.name(), func);
    fns
}
