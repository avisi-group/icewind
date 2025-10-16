use {
    crate::host::dbt::emitter::Emitter,
    core::{marker::PhantomData, sync::atomic::AtomicUsize},
 brig_common::Alloc,
};

pub struct Dag<A> {
    alloc: PhantomData<A>,
    next_id: AtomicUsize,
}

impl<A: Alloc> Dag<A> {
    pub fn new() -> Self {
        Self {
            alloc: PhantomData::default(),
            next_id: AtomicUsize::new(0),
        }
    }

    pub fn get_next_id(&self) -> usize {
        self.next_id
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed)
    }

    fn new_node(&self, kind: DagNodeKind) -> DagNode {
        DagNode {
            id: self.get_next_id(),
            kind,
        }
    }
}

pub struct DagNode {
    id: usize,
    kind: DagNodeKind,
}

pub struct DagBlock {
    id: usize,
}

pub enum DagUnOpKind {
    Not,
}

pub enum DagBinOpKind {
    Add,
}

pub enum DagTernOpKind {
    Select,
}

pub enum DagCastKind {
    ZeroExtend,
}

pub enum DagShiftKind {
    Left,
    Right,
    ArithmeticRight,
}

pub enum DagNodeKind {
    Constant(u64),
    FunctionPointer(u64),
    CreateBits {
        value: usize,
        length: usize,
    },
    SizeOf {
        value: usize,
    },
    CreateTuple {
        values: alloc::vec::Vec<usize>,
    },
    AccessTuple {
        tuple: usize,
        index: usize,
    },
    UnOp {
        kind: DagUnOpKind,
        value: usize,
    },
    BinOp {
        kind: DagBinOpKind,
        lhs: usize,
        rhs: usize,
    },
    TernOp {
        kind: DagTernOpKind,
        a: usize,
        b: usize,
        c: usize,
    },
    Cast {
        kind: DagCastKind,
        value: usize,
    },
    BitsCast {
        kind: DagCastKind,
        length: usize,
        value: usize,
    },
    Shift {
        kind: DagShiftKind,
        value: usize,
        amount: usize,
    },
    BitExtract {
        value: usize,
        offset: usize,
        length: usize,
    },
    BitInsert {
        original: usize,
        value: usize,
        offset: usize,
        length: usize,
    },
    BitReplicate {
        pattern: usize,
        count: usize,
    },
    Select {
        condition: usize,
        true_value: usize,
        false_value: usize,
    },
    GetFlags {
        expression: usize,
    },
    ReadRegister {
        offset: usize,
    },
    ReadMemory {
        offset: usize,
    },
    ReadVariable {
        id: usize,
    },
    Call {
        func: usize,
    },
}

impl<A: Alloc> Emitter<A> for Dag<A> {
    type BlockRef = ();

    type NodeRef = DagNode;

    fn constant(&mut self, val: u64, typ: super::emitter::Type) -> Self::NodeRef {
        self.new_node(DagNodeKind::Constant(val))
    }

    fn function_ptr(&mut self, val: u64) -> Self::NodeRef {
        self.new_node(DagNodeKind::FunctionPointer(val))
    }

    fn create_bits(&mut self, value: Self::NodeRef, length: Self::NodeRef) -> Self::NodeRef {
        self.new_node(DagNodeKind::CreateBits {
            value: value.id,
            length: length.id,
        })
    }

    fn size_of(&mut self, value: Self::NodeRef) -> Self::NodeRef {
        self.new_node(DagNodeKind::SizeOf { value: value.id })
    }

    fn create_tuple(&mut self, values: alloc::vec::Vec<Self::NodeRef, A>) -> Self::NodeRef {
        self.new_node(DagNodeKind::CreateTuple {
            values: values.iter().map(|v| v.id).collect(),
        })
    }

    fn access_tuple(&mut self, tuple: Self::NodeRef, index: usize) -> Self::NodeRef {
        self.new_node(DagNodeKind::AccessTuple {
            tuple: tuple.id,
            index,
        })
    }

    fn unary_operation(&mut self, op: super::x86::emitter::UnaryOperationKind<A>) -> Self::NodeRef {
        // match op {
        //         super::x86::emitter::UnaryOperationKind::Not(x86_node_ref) =>
        // self.new_node(DagNodeKind::UnOp { kind: DagUnOpKind::Not, value: x86 })
        //         super::x86::emitter::UnaryOperationKind::Negate(x86_node_ref) =>
        // todo!(),
        //         super::x86::emitter::UnaryOperationKind::Complement(x86_node_ref) =>
        // todo!(),
        //         super::x86::emitter::UnaryOperationKind::Power2(x86_node_ref) =>
        // todo!(),
        //         super::x86::emitter::UnaryOperationKind::Absolute(x86_node_ref) =>
        // todo!(),
        //         super::x86::emitter::UnaryOperationKind::Ceil(x86_node_ref) =>
        // todo!(),
        //         super::x86::emitter::UnaryOperationKind::Floor(x86_node_ref) =>
        // todo!(),
        //         super::x86::emitter::UnaryOperationKind::SquareRoot(x86_node_ref) =>
        // todo!(),     }

        // self.new_node(DagNodeKind::UnOp {
        //     kind: ,
        //     value: (),
        // })
        todo!()
    }

    fn binary_operation(
        &mut self,
        op: super::x86::emitter::BinaryOperationKind<A>,
    ) -> Self::NodeRef {
        todo!()
    }

    fn ternary_operation(
        &mut self,
        op: super::x86::emitter::TernaryOperationKind<A>,
    ) -> Self::NodeRef {
        todo!()
    }

    fn cast(
        &mut self,
        value: Self::NodeRef,
        typ: super::emitter::Type,
        kind: super::x86::emitter::CastOperationKind,
    ) -> Self::NodeRef {
        todo!()
    }

    fn bits_cast(
        &mut self,
        value: Self::NodeRef,
        length: Self::NodeRef,
        typ: super::emitter::Type,
        kind: super::x86::emitter::CastOperationKind,
    ) -> Self::NodeRef {
        todo!()
    }

    fn shift(
        &mut self,
        value: Self::NodeRef,
        amount: Self::NodeRef,
        kind: super::x86::emitter::ShiftOperationKind,
    ) -> Self::NodeRef {
        todo!()
    }

    fn bit_extract(
        &mut self,
        value: Self::NodeRef,
        start: Self::NodeRef,
        length: Self::NodeRef,
    ) -> Self::NodeRef {
        todo!()
    }

    fn bit_insert(
        &mut self,
        target: Self::NodeRef,
        source: Self::NodeRef,
        start: Self::NodeRef,
        length: Self::NodeRef,
    ) -> Self::NodeRef {
        todo!()
    }

    fn bit_replicate(&mut self, pattern: Self::NodeRef, count: Self::NodeRef) -> Self::NodeRef {
        todo!()
    }

    fn select(
        &mut self,
        condition: Self::NodeRef,
        true_value: Self::NodeRef,
        false_value: Self::NodeRef,
    ) -> Self::NodeRef {
        todo!()
    }

    fn assert(&mut self, condition: Self::NodeRef, metadata: u64) {
        todo!()
    }

    fn get_flags(&mut self, operation: Self::NodeRef) -> Self::NodeRef {
        todo!()
    }

    fn read_register(&mut self, offset: u64, typ: super::emitter::Type) -> Self::NodeRef {
        todo!()
    }

    fn write_register(&mut self, offset: u64, value: Self::NodeRef) {
        todo!()
    }

    fn read_memory(&mut self, address: Self::NodeRef, typ: super::emitter::Type) -> Self::NodeRef {
        todo!()
    }

    fn write_memory(
        &mut self,
        address: Self::NodeRef,
        value: Self::NodeRef,
        is_unprivileged: bool,
    ) {
        todo!()
    }

    fn read_stack_variable(&mut self, id: usize, typ: super::emitter::Type) -> Self::NodeRef {
        todo!()
    }

    fn write_stack_variable(&mut self, id: usize, value: Self::NodeRef) {
        todo!()
    }

    fn panic(&mut self, msg: &str) {
        todo!()
    }

    fn branch(
        &mut self,
        condition: Self::NodeRef,
        true_target: Self::BlockRef,
        false_target: Self::BlockRef,
    ) {
        todo!()
    }

    fn jump(&mut self, target: Self::BlockRef) {
        todo!()
    }

    fn call(&mut self, function: Self::NodeRef, arguments: alloc::vec::Vec<Self::NodeRef, A>) {
        todo!()
    }

    fn call_with_return(
        &mut self,
        function: Self::NodeRef,
        arguments: alloc::vec::Vec<Self::NodeRef, A>,
    ) -> Self::NodeRef {
        todo!()
    }

    fn prologue(&mut self) {
        todo!()
    }

    fn leave(&mut self) {
        todo!()
    }

    fn leave_with_cache(&mut self, chain_cache: u64) {
        todo!()
    }

    fn set_current_block(&mut self, block: Self::BlockRef) {
        todo!()
    }

    fn get_current_block(&self) -> Self::BlockRef {
        todo!()
    }
}
