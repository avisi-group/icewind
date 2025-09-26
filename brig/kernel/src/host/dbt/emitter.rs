use {
    crate::host::dbt::{
        Alloc,
        x86::emitter::{
            BinaryOperationKind, CastOperationKind, ShiftOperationKind, TernaryOperationKind,
            UnaryOperationKind,
        },
    },
    alloc::vec::Vec,
};

pub trait Emitter<A: Alloc> {
    type BlockRef;
    type NodeRef;

    fn constant(&mut self, val: u64, typ: Type) -> Self::NodeRef;
    fn function_ptr(&mut self, val: u64) -> Self::NodeRef;
    fn create_bits(&mut self, value: Self::NodeRef, length: Self::NodeRef) -> Self::NodeRef;
    fn size_of(&mut self, value: Self::NodeRef) -> Self::NodeRef;
    fn create_tuple(&mut self, values: Vec<Self::NodeRef, A>) -> Self::NodeRef;
    fn access_tuple(&mut self, tuple: Self::NodeRef, index: usize) -> Self::NodeRef;

    fn unary_operation(&mut self, op: UnaryOperationKind<A>) -> Self::NodeRef;
    fn binary_operation(&mut self, op: BinaryOperationKind<A>) -> Self::NodeRef;
    fn ternary_operation(&mut self, op: TernaryOperationKind<A>) -> Self::NodeRef;
    fn cast(&mut self, value: Self::NodeRef, typ: Type, kind: CastOperationKind) -> Self::NodeRef;
    fn bits_cast(
        &mut self,
        value: Self::NodeRef,
        length: Self::NodeRef,
        typ: Type,
        kind: CastOperationKind,
    ) -> Self::NodeRef;
    fn shift(
        &mut self,
        value: Self::NodeRef,
        amount: Self::NodeRef,
        kind: ShiftOperationKind,
    ) -> Self::NodeRef;

    fn bit_extract(
        &mut self,
        value: Self::NodeRef,
        start: Self::NodeRef,
        length: Self::NodeRef,
    ) -> Self::NodeRef;

    fn bit_insert(
        &mut self,
        target: Self::NodeRef,
        source: Self::NodeRef,
        start: Self::NodeRef,
        length: Self::NodeRef,
    ) -> Self::NodeRef;

    fn bit_replicate(&mut self, pattern: Self::NodeRef, count: Self::NodeRef) -> Self::NodeRef;

    fn select(
        &mut self,
        condition: Self::NodeRef,
        true_value: Self::NodeRef,
        false_value: Self::NodeRef,
    ) -> Self::NodeRef;

    fn assert(&mut self, condition: Self::NodeRef, metadata: u64);

    fn get_flags(&mut self, operation: Self::NodeRef) -> Self::NodeRef;

    fn read_register(&mut self, offset: u64, typ: Type) -> Self::NodeRef;
    fn write_register(&mut self, offset: u64, value: Self::NodeRef);

    fn read_memory(&mut self, address: Self::NodeRef, typ: Type) -> Self::NodeRef;
    fn write_memory(&mut self, address: Self::NodeRef, value: Self::NodeRef, is_unprivileged: bool);

    fn read_stack_variable(&mut self, id: usize, typ: Type) -> Self::NodeRef;
    fn write_stack_variable(&mut self, id: usize, value: Self::NodeRef);

    fn panic(&mut self, msg: &str);

    fn branch(
        &mut self,
        condition: Self::NodeRef,
        true_target: Self::BlockRef,
        false_target: Self::BlockRef,
    );

    fn jump(&mut self, target: Self::BlockRef);

    fn call(&mut self, function: Self::NodeRef, arguments: Vec<Self::NodeRef, A>);

    fn call_with_return(
        &mut self,
        function: Self::NodeRef,
        arguments: Vec<Self::NodeRef, A>,
    ) -> Self::NodeRef;

    fn prologue(&mut self);

    // cleanup and return
    fn leave(&mut self);
    fn leave_with_cache(&mut self, chain_cache: u64);

    fn set_current_block(&mut self, block: Self::BlockRef);
    fn get_current_block(&self) -> Self::BlockRef;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Unsigned(u32),
    Signed(u32),
    Floating(u32),
    Bits,
    Tuple,
}

impl Type {
    pub fn width(&self) -> u32 {
        match self {
            Type::Unsigned(w) | Type::Signed(w) | Type::Floating(w) => *w,
            Type::Bits => 64, // todo: should this be the runtime length?
            Type::Tuple => todo!(),
        }
    }
}

// pub struct WrappedEmitter<E: Emitter> {
//     subemitter: E,
// }

// impl<E: Emitter> WrappedEmitter<E> {
//     pub fn new(subemitter: E) -> Self {
//         Self { subemitter }
//     }
// }

// impl<E: Emitter> Emitter for WrappedEmitter<E> {
//     type BlockRef = E::BlockRef;
//     type NodeRef = E::NodeRef;
//     type SymbolRef = E::SymbolRef;

//     fn constant(&mut self, val: u64, typ: Type) -> Self::NodeRef {
//         log::info!("constant {}", val);
//         self.subemitter.constant(val, typ)
//     }

//     fn read_register(&mut self, offset: Self::NodeRef, typ: Type) ->
// Self::NodeRef {         log::info!("read-reg");
//         self.subemitter.read_register(offset, typ)
//     }

//     fn write_register(&mut self, offset: Self::NodeRef, value: Self::NodeRef)
// {         log::info!("write-reg");
//         self.subemitter.write_register(offset, value);
//     }

//     fn read_variable(&mut self, symbol: Self::SymbolRef) -> Self::NodeRef {
//         log::info!("read-var");
//         self.subemitter.read_variable(symbol)
//     }

//     fn write_variable(&mut self, symbol: Self::SymbolRef, value:
// Self::NodeRef) {         log::info!("write-var");
//         self.subemitter.write_variable(symbol, value);
//     }

//     fn branch(
//         &mut self,
//         condition: Self::NodeRef,
//         true_target: Self::BlockRef,
//         false_target: Self::BlockRef,
//     ) -> BlockResult {
//         log::info!("branch");
//         self.subemitter.branch(condition, true_target, false_target)
//     }

//     fn jump(&mut self, target: Self::BlockRef) -> BlockResult {
//         log::info!("jump");
//         self.subemitter.jump(target)
//     }

//     fn leave(&mut self) {
//         log::info!("leave");
//         self.subemitter.leave();
//     }

//     fn set_current_block(&mut self, block: Self::BlockRef) {
//         self.subemitter.set_current_block(block);
//     }

//     fn unary_operation(&mut self, op: UnaryOperationKind) -> Self::NodeRef {
//         log::info!("un-op");
//         self.subemitter.unary_operation(op)
//     }

//     fn binary_operation(&mut self, op: BinaryOperationKind) -> Self::NodeRef
// {         log::info!("bin-op");
//         self.subemitter.binary_operation(op)
//     }

//     fn cast(&mut self, value: Self::NodeRef, typ: Type, kind:
// CastOperationKind) -> Self::NodeRef {         log::info!("cast");
//         self.subemitter.cast(value, typ, kind)
//     }

//     fn shift(
//         &mut self,
//         value: Self::NodeRef,
//         amount: Self::NodeRef,
//         kind: ShiftOperationKind,
//     ) -> Self::NodeRef {
//         log::info!("shift");
//         self.subemitter.shift(value, amount, kind)
//     }

//     fn bit_extract(
//         &mut self,
//         value: Self::NodeRef,
//         start: Self::NodeRef,
//         length: Self::NodeRef,
//     ) -> Self::NodeRef {
//         log::info!("bit-extract");
//         self.subemitter.bit_extract(value, start, length)
//     }

//     fn select(
//         &mut self,
//         condition: Self::NodeRef,
//         true_value: Self::NodeRef,
//         false_value: Self::NodeRef,
//     ) -> Self::NodeRef {
//         log::info!("select");
//         self.subemitter.select(condition, true_value, false_value)
//     }

//     fn assert(&mut self, condition: Self::NodeRef) {
//         log::info!("select");
//         todo!()
//     }
// }
