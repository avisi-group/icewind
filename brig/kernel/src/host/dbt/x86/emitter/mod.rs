use {
    crate::{
        guest::GuestExecutionContext,
        host::dbt::{
            Alloc, bit_extract, bit_insert,
            emitter::Type,
            models::{CHAIN_CACHE_ENTRY_COUNT, write_to_el},
            trampoline::ExecutionResult,
            x86::{
                Emitter, X86TranslationContext,
                encoder::{
                    Instruction, MemoryScale, Opcode, Operand, OperandKind,
                    registers::{PhysicalRegister, Register, SegmentRegister},
                    width::Width,
                },
                register_allocator::RegisterAllocator,
            },
        },
    },
    aarch64_paging::target,
    alloc::{rc::Rc, vec::Vec},
    common::{arena::Ref, hashmap::HashMap, mask::mask},
    core::{
        cmp::{Ordering, min},
        fmt::Debug,
        hash::{Hash, Hasher},
        mem::offset_of,
        ops::RangeBounds,
        panic,
    },
    derive_where::derive_where,
    proc_macro_lib::ktest,
};

mod to_operand;

const INVALID_OFFSET: i32 = 0xDEAD00F;

// sysv64 ABI
pub const ARG_REGS: &[PhysicalRegister] = &[
    PhysicalRegister::RDI,
    PhysicalRegister::RSI,
    PhysicalRegister::RDX,
    PhysicalRegister::RCX,
    PhysicalRegister::R8,
    PhysicalRegister::R9,
];

pub const CALLER_SAVED: &[PhysicalRegister] = &[
    PhysicalRegister::RAX,
    PhysicalRegister::RDI,
    PhysicalRegister::RSI,
    PhysicalRegister::RDX,
    PhysicalRegister::RCX,
    PhysicalRegister::R8,
    PhysicalRegister::R9,
    PhysicalRegister::R10,
    PhysicalRegister::R11,
];

/// X86 emitter error
#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum X86Error {
    /// Left and right types do not match in binary operation
    BinaryOperationTypeMismatch,
    /// Register allocation failed
    RegisterAllocation,
}

pub struct X86Emitter<'ctx, A: Alloc> {
    current_block: Ref<X86Block<A>>,
    current_block_operands: HashMap<X86NodeRef<A>, Operand<A>>,
    panic_block: Ref<X86Block<A>>,
    next_vreg: usize,
    pub execution_result: ExecutionResult,
    ctx: &'ctx mut X86TranslationContext<A>,
    // node to global variable ID
    sets_flags: HashMap<X86NodeRef<A>, usize>,
}

impl<'a, 'ctx, A: Alloc> X86Emitter<'ctx, A> {
    pub fn new(ctx: &'ctx mut X86TranslationContext<A>) -> Self {
        Self {
            current_block: ctx.initial_block(),
            current_block_operands: HashMap::default(),
            panic_block: ctx.panic_block(),
            next_vreg: 0,
            execution_result: ExecutionResult::new(),
            ctx,
            sets_flags: HashMap::default(),
        }
    }

    pub fn ctx(&self) -> &X86TranslationContext<A> {
        &self.ctx
    }

    pub fn ctx_mut(&mut self) -> &mut X86TranslationContext<A> {
        &mut self.ctx
    }

    pub fn node(&self, node: X86Node<A>) -> X86NodeRef<A> {
        X86NodeRef(Rc::new_in(node, self.ctx().allocator.clone()))
    }

    pub fn next_vreg(&mut self) -> usize {
        let vreg = self.next_vreg;
        self.next_vreg += 1;
        vreg
    }

    pub fn push_instruction(&mut self, instr: Instruction<A>) {
        self.current_block
            .get_mut(self.ctx.arena_mut())
            .append(instr);
    }

    pub fn push_target(&mut self, target: Ref<X86Block<A>>) {
        log::debug!("adding target {target:?} to {:?}", self.current_block);
        self.current_block
            .get_mut(self.ctx.arena_mut())
            .push_next(target);
    }

    fn emit_call(
        &mut self,
        function: X86NodeRef<A>,
        arguments: Vec<X86NodeRef<A>, A>,
        has_return_value: bool,
    ) {
        let function = self.to_operand_reg_promote(&function);

        let arg_count = arguments.len();

        arguments
            .into_iter()
            .map(|arg| self.to_operand(&arg))
            .collect::<Vec<_>>()
            .into_iter()
            .zip(ARG_REGS.iter())
            .for_each(|(src, dst)| {
                self.push_instruction(
                    Instruction::mov(src, Operand::preg(Width::_64, *dst)).unwrap(),
                )
            });

        // to be replaced with pushes and pops if necessary, without breaking all jump
        // indexes
        for _ in 0..CALLER_SAVED.len() {
            self.push_instruction(Instruction(Opcode::DEAD));
        }

        self.push_instruction(Instruction::call(
            function,
            arg_count,
            if has_return_value { 1 } else { 0 },
        ));

        for _ in 0..CALLER_SAVED.len() {
            self.push_instruction(Instruction(Opcode::DEAD));
        }
    }
}

impl<'ctx, A: Alloc> Emitter<A> for X86Emitter<'ctx, A> {
    type NodeRef = X86NodeRef<A>;
    type BlockRef = Ref<X86Block<A>>;

    fn set_current_block(&mut self, block: Self::BlockRef) {
        self.current_block = block;
        self.current_block_operands = HashMap::default();
    }

    fn get_current_block(&self) -> Self::BlockRef {
        self.current_block
    }

    fn constant(&mut self, value: u64, typ: Type) -> Self::NodeRef {
        let width = typ.width();
        if width == 0 {
            panic!(
                "no zero width constants allowed! {typ:?} @ {:?}",
                self.current_block
            )
        }
        self.node(X86Node {
            typ,
            kind: NodeKind::Constant { value, width },
        })
    }

    fn function_ptr(&mut self, val: u64) -> Self::NodeRef {
        self.node(X86Node {
            typ: Type::Unsigned(64),
            kind: NodeKind::FunctionPointer(val),
        })
    }

    // may not return a bits if `length` is a constant?
    fn create_bits(&mut self, value: Self::NodeRef, length: Self::NodeRef) -> Self::NodeRef {
        // evil bits that's really a fixed unsigned pretending to be a bitvector
        if let NodeKind::Constant { value: length, .. } = length.kind() {
            let length = u32::try_from(*length).unwrap();
            let target_type = match value.typ() {
                Type::Unsigned(_) => Type::Unsigned(length),
                Type::Signed(_) => Type::Signed(length),
                _ => todo!(),
            };

            let kind = match value.typ().width().cmp(&length) {
                Ordering::Less => CastOperationKind::ZeroExtend,
                Ordering::Equal => CastOperationKind::Reinterpret,
                Ordering::Greater => CastOperationKind::Truncate,
            };

            self.cast(value, target_type, kind)
        } else {
            // todo: attach length information
            value
        }
    }

    fn read_register(&mut self, offset: u64, typ: Type) -> Self::NodeRef {
        self.node(X86Node {
            typ,
            kind: NodeKind::GuestRegister { offset },
        })
    }

    fn unary_operation(&mut self, op: UnaryOperationKind<A>) -> Self::NodeRef {
        use UnaryOperationKind::*;

        match &op {
            Not(value) => match value.kind() {
                NodeKind::Constant {
                    value: constant_value,
                    width,
                } => self.node(X86Node {
                    typ: value.typ().clone(),
                    kind: NodeKind::Constant {
                        value: (*constant_value == 0) as u64,
                        width: *width,
                    },
                }),
                _ => self.node(X86Node {
                    typ: value.typ().clone(),
                    kind: NodeKind::UnaryOperation(op),
                }),
            },
            Complement(value) => {
                match value.kind() {
                    NodeKind::Constant {
                        value: constant_value,
                        width,
                    } => self.node(X86Node {
                        typ: value.typ().clone(),
                        kind: NodeKind::Constant {
                            value: (!constant_value) & mask(*width), /* only invert the bits that
                                                                      * are
                                                                      * part of the size of the
                                                                      * datatype */
                            width: *width,
                        },
                    }),
                    _ => self.node(X86Node {
                        typ: value.typ().clone(),
                        kind: NodeKind::UnaryOperation(op),
                    }),
                }
            }
            Ceil(value) => {
                let NodeKind::Tuple(values) = value.kind() else {
                    panic!()
                };

                if values
                    .iter()
                    .all(|v| matches!(v.kind(), NodeKind::Constant { .. }))
                {
                    todo!()
                } else {
                    self.node(X86Node {
                        typ: Type::Signed(64),
                        kind: NodeKind::UnaryOperation(op),
                    })
                }
            }

            Floor(value) => {
                let NodeKind::Tuple(values) = value.kind() else {
                    panic!()
                };

                if values
                    .iter()
                    .all(|v| matches!(v.kind(), NodeKind::Constant { .. }))
                {
                    let [num, den] = values.as_slice() else {
                        panic!()
                    };

                    assert_eq!(num.typ(), Type::Signed(64));
                    assert_eq!(den.typ(), Type::Signed(64));

                    let (
                        NodeKind::Constant { value: num, .. },
                        NodeKind::Constant { value: den, .. },
                    ) = (num.kind(), den.kind())
                    else {
                        panic!()
                    };

                    let num = *num as i64;
                    let den = *den as i64;

                    let value = num.div_floor(den) as u64;

                    self.node(X86Node {
                        typ: Type::Signed(64),
                        kind: NodeKind::Constant { value, width: 64 },
                    })
                } else {
                    self.node(X86Node {
                        typ: Type::Signed(64),
                        kind: NodeKind::UnaryOperation(op),
                    })
                }
            }

            Negate(value) => match value.kind() {
                NodeKind::Constant {
                    value: const_value,
                    width: _const_width,
                } => match value.typ() {
                    Type::Signed(type_width) => {
                        let negated = (-(*const_value as i64)) as u64;
                        self.constant(negated, Type::Signed(type_width))
                    }
                    _ => todo!(),
                },
                _ => self.node(X86Node {
                    typ: value.typ().clone(),
                    kind: NodeKind::UnaryOperation(op),
                }),
            },

            _ => {
                todo!("{op:?}")
            }
        }
    }

    fn binary_operation(&mut self, op: BinaryOperationKind<A>) -> Self::NodeRef {
        use BinaryOperationKind::*;

        // todo: re-enable me
        // match &op {
        //     Add(lhs, rhs)
        //     | Sub(lhs, rhs)
        //     | Multiply(lhs, rhs)
        //     | Divide(lhs, rhs)
        //     | Modulo(lhs, rhs)
        //     | Or(lhs, rhs)
        //     | Xor(lhs, rhs)
        //     | And(lhs, rhs)
        //     | PowI(lhs, rhs)
        //     | CompareEqual(lhs, rhs)
        //     | CompareNotEqual(lhs, rhs)
        //     | CompareLessThan(lhs, rhs)
        //     | CompareLessThanOrEqual(lhs, rhs)
        //     | CompareGreaterThan(lhs, rhs)
        //     | CompareGreaterThanOrEqual(lhs, rhs) => {
        //         if lhs.typ() != rhs.typ() {
        //             return Err(X86Error::BinaryOperationTypeMismatch { op: op.clone()
        // });         }
        //     }
        // }

        match &op {
            Add(lhs, rhs) => match (lhs.kind(), rhs.kind()) {
                (
                    NodeKind::Constant {
                        value: lhs_value,
                        width,
                    },
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: lhs_value.wrapping_add(*rhs_value),// todo: THIS WILL WRAP AT 64 NOT *width*!
                        width: *width,
                    },
                }),
                (
                    NodeKind::Constant {
                        value: lhs_value, ..
                    },
                    _,
                ) => {
                    if *lhs_value == 0 {
                        rhs.clone()
                    } else {
                        self.node(X86Node {
                            typ: lhs.typ().clone(),
                            kind: NodeKind::BinaryOperation(op),
                        })
                    }
                }
                (
                    _,
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => {
                    if *rhs_value == 0 {
                        lhs.clone()
                    } else {
                        self.node(X86Node {
                            typ: lhs.typ().clone(),
                            kind: NodeKind::BinaryOperation(op),
                        })
                    }
                }
                _ => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::BinaryOperation(op),
                }),
            },
            Sub(lhs, rhs) => match (lhs.kind(), rhs.kind()) {
                (
                    NodeKind::Constant {
                        value: lhs_value,
                        width,
                    },
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: lhs_value.wrapping_sub(*rhs_value),// todo: THIS WILL WRAP AT 64 NOT *width*!
                        width: *width,
                    },
                }),
                _ => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::BinaryOperation(op),
                }),
            },
            Multiply(lhs, rhs) => match (lhs.kind(), rhs.kind()) {
                (
                    NodeKind::Constant {
                        value: lhs_value,
                        width,
                    },
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: lhs_value * rhs_value,
                        width: *width,
                    },
                }),
                (NodeKind::Constant { value: 0, .. }, _) => self.node(X86Node {
                    typ: rhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: 0,
                        width: rhs.typ().width(),
                    },
                }),
                (NodeKind::Constant { value: 1, .. }, _) => rhs.clone(),
                (_, NodeKind::Constant { value: 0, .. }) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: 0,
                        width: lhs.typ().width(),
                    },
                }),
                (_, NodeKind::Constant { value: 1, .. }) => lhs.clone(),
                _ => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::BinaryOperation(op),
                }),
            },

            Divide(lhs, rhs) => match (lhs.kind(), rhs.kind()) {
                (
                    NodeKind::Constant {
                        value: lhs_value,
                        width,
                    },
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: lhs_value / rhs_value,
                        width: *width,
                    },
                }),
                (NodeKind::Tuple(left), NodeKind::Tuple(right)) => {
                    match (left.as_slice(), right.as_slice()) {
                        ([left_num, left_den], [right_num, right_den]) => {
                            let num = self.binary_operation(BinaryOperationKind::Multiply(
                                left_num.clone(),
                                right_den.clone(),
                            ));
                            let den = self.binary_operation(BinaryOperationKind::Multiply(
                                left_den.clone(),
                                right_num.clone(),
                            ));
                            let mut tuple = Vec::new_in(self.ctx().allocator());
                            tuple.push(num);
                            tuple.push(den);

                            self.create_tuple(tuple)
                        }
                        _ => panic!(),
                    }
                }
                _ => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::BinaryOperation(op),
                }),
            },
            Modulo(lhs, rhs) => match (lhs.kind(), rhs.kind()) {
                (
                    NodeKind::Constant {
                        value: lhs_value,
                        width,
                    },
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: lhs_value % rhs_value,
                        width: *width,
                    },
                }),
                _ => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::BinaryOperation(op),
                }),
            },
            Or(lhs, rhs) => match (lhs.kind(), rhs.kind()) {
                (
                    NodeKind::Constant {
                        value: lhs_value,
                        width,
                    },
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: lhs_value | rhs_value,
                        width: *width,
                    },
                }),
                _ => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::BinaryOperation(op),
                }),
            },
            Xor(lhs, rhs) => match (lhs.kind(), rhs.kind()) {
                (
                    NodeKind::Constant {
                        value: lhs_value,
                        width,
                    },
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: lhs_value ^ rhs_value,
                        width: *width,
                    },
                }),
                _ => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::BinaryOperation(op),
                }),
            },
            And(lhs, rhs) => match (lhs.kind(), rhs.kind()) {
                (
                    NodeKind::Constant {
                        value: lhs_value,
                        width,
                    },
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: lhs_value & rhs_value,
                        width: *width,
                    },
                }),
                (
                    NodeKind::Constant {
                        value: lhs_value, ..
                    },
                    ..,
                ) => {
                    if *lhs_value == 0 {
                        self.constant(0, rhs.typ())
                    } else if is_no_op_and(rhs.typ(), lhs.typ(), *lhs_value) {
                        rhs.clone()
                    } else {
                        self.node(X86Node {
                            typ: lhs.typ().clone(),
                            kind: NodeKind::BinaryOperation(op),
                        })
                    }
                }
                (
                    ..,
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => {
                    if *rhs_value == 0 {
                        self.constant(0, lhs.typ())
                    } else if is_no_op_and(lhs.typ(), rhs.typ(), *rhs_value) {
                        lhs.clone()
                    } else {
                        self.node(X86Node {
                            typ: rhs.typ().clone(),
                            kind: NodeKind::BinaryOperation(op),
                        })
                    }
                }
                _ => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::BinaryOperation(op),
                }),
            },
            CompareEqual(lhs, rhs) => match (lhs.kind(), rhs.kind()) {
                (
                    NodeKind::Constant {
                        value: lhs_value, ..
                    },
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: if lhs_value == rhs_value { 1 } else { 0 },
                        width: 1,
                    },
                }),
                _ => self.node(X86Node {
                    typ: Type::Unsigned(1),
                    kind: NodeKind::BinaryOperation(op),
                }),
            },
            CompareNotEqual(lhs, rhs) => match (lhs.kind(), rhs.kind()) {
                (
                    NodeKind::Constant {
                        value: lhs_value, ..
                    },
                    NodeKind::Constant {
                        value: rhs_value, ..
                    },
                ) => self.node(X86Node {
                    typ: lhs.typ().clone(),
                    kind: NodeKind::Constant {
                        value: if lhs_value != rhs_value { 1 } else { 0 },
                        width: 1,
                    },
                }),
                _ => self.node(X86Node {
                    typ: Type::Unsigned(1),
                    kind: NodeKind::BinaryOperation(op),
                }),
            },

            CompareGreaterThan(_, _)
            | CompareGreaterThanOrEqual(_, _)
            | CompareLessThan(_, _)
            | CompareLessThanOrEqual(_, _) => emit_compare(op, self),

            op => {
                todo!("{op:?}")
            }
        }
    }

    fn ternary_operation(&mut self, op: TernaryOperationKind<A>) -> Self::NodeRef {
        use TernaryOperationKind::*;
        match &op {
            AddWithCarry(src, dst, carry) => {
                // todo: fix this
                // // if any are const, let add infrastructure handle this
                // if matches!(src.kind(), NodeKind::Constant { .. })
                //     || matches!(dst.kind(), NodeKind::Constant { .. })
                //     || matches!(carry.kind(), NodeKind::Constant { .. })
                // {
                //     let carry = self.cast(carry.clone(), dst.typ(),
                // CastOperationKind::ZeroExtend);     let src =
                // self.cast(src.clone(), dst.typ(), CastOperationKind::Reinterpret);

                //     let sum =
                //         self.binary_operation(BinaryOperationKind::Add(src.clone(),
                // dst.clone()));

                //     self.binary_operation(BinaryOperationKind::Add(carry, sum.clone()))
                // } else {
                //     self.node(X86Node {
                //         typ: src.typ().clone(),
                //         kind: NodeKind::TernaryOperation(op),
                //     })
                // }

                self.node(X86Node {
                    typ: src.typ().clone(),
                    kind: NodeKind::TernaryOperation(op),
                })
            }
        }
    }

    fn cast(
        &mut self,
        value: Self::NodeRef,
        target_type: Type,
        cast_kind: CastOperationKind,
    ) -> Self::NodeRef {
        match value.kind() {
            NodeKind::Constant {
                value: constant_value,
                ..
            } => {
                if let Type::Bits = target_type {
                    panic!("don't cast to a bits:(")
                }

                let original_width = value.typ().width();
                let target_width = target_type.width();

                let casted_value = match cast_kind {
                    CastOperationKind::ZeroExtend => {
                        if original_width == 64 {
                            *constant_value
                        } else {
                            // extending from the incoming value type - so can clear
                            // all upper bits.
                            let mask = mask(original_width);
                            *constant_value & mask
                        }
                    }
                    CastOperationKind::SignExtend => {
                        sign_extend(*constant_value, original_width, target_width)
                    }
                    CastOperationKind::Truncate => {
                        // truncating to the target width - just clear all irrelevant bits
                        let mask = mask(target_width);
                        *constant_value & mask
                    }
                    CastOperationKind::Reinterpret => *constant_value,
                    CastOperationKind::Convert => *constant_value,
                    CastOperationKind::Broadcast => *constant_value,
                };

                self.constant(casted_value, target_type)
            }
            _ => match cast_kind {
                CastOperationKind::Reinterpret | CastOperationKind::Truncate => {
                    if value.typ() == target_type {
                        value
                    } else {
                        self.node(X86Node {
                            typ: target_type,
                            kind: NodeKind::Cast {
                                value,
                                kind: cast_kind,
                            },
                        })
                    }
                }
                _ => self.node(X86Node {
                    typ: target_type,
                    kind: NodeKind::Cast {
                        value,
                        kind: cast_kind,
                    },
                }),
            },
        }
    }

    fn shift(
        &mut self,
        value: Self::NodeRef,
        amount: Self::NodeRef,
        kind: ShiftOperationKind,
    ) -> Self::NodeRef {
        let typ = value.typ().clone();
        match (value.kind(), amount.kind(), kind.clone()) {
            (
                NodeKind::Constant {
                    value: value_value,
                    width: value_width,
                },
                NodeKind::Constant {
                    value: amount_value,
                    ..
                },
                ShiftOperationKind::LogicalShiftLeft,
            ) => {
                let shifted = match (value_value, amount_value) {
                    (0, _) => 0,
                    (v, 0) => *v,
                    (v, a) => v
                        .checked_shl(u32::try_from(*a).unwrap())
                        .unwrap_or_else(|| {
                            log::warn!("failed to shift left {value:?} by {amount:?}");
                            0
                        }),
                };

                // shift and mask to width of value
                self.constant(shifted & mask(*value_width), typ)
            }
            (
                NodeKind::Constant {
                    value: value_value, ..
                },
                NodeKind::Constant {
                    value: amount_value,
                    ..
                },
                ShiftOperationKind::LogicalShiftRight,
            ) => {
                // mask to width of value
                self.constant(
                    value_value
                        .checked_shr(u32::try_from(*amount_value).unwrap())
                        .unwrap_or(0),
                    typ,
                )
            }
            (
                NodeKind::Constant {
                    value: value_value,
                    width: 64, // has to be 64 for the i64 shift to be valid
                },
                NodeKind::Constant {
                    value: amount_value,
                    ..
                },
                ShiftOperationKind::ArithmeticShiftRight,
            ) => {
                let signed_value = *value_value as i64;
                let shifted = signed_value
                    .checked_shr(u32::try_from(*amount_value).unwrap())
                    .unwrap() as u64;

                // mask to width of value
                self.constant(shifted, typ)
            }
            (
                NodeKind::Constant {
                    value: value_value,
                    width: 32,
                },
                NodeKind::Constant {
                    value: amount_value,
                    ..
                },
                ShiftOperationKind::ArithmeticShiftRight,
            ) => {
                let signed_value = *value_value as i32;
                let shifted = signed_value
                    .checked_shr(u32::try_from(*amount_value).unwrap())
                    .unwrap() as u64;

                // mask to width of value
                self.constant(shifted, typ)
            }
            (NodeKind::Constant { .. }, NodeKind::Constant { .. }, k) => {
                todo!("{k:?}")
            }
            (_, NodeKind::Constant { value: 0, .. }, _) => value,
            (_, _, _) => self.node(X86Node {
                typ,
                kind: NodeKind::Shift {
                    value,
                    amount,
                    kind,
                },
            }),
        }
    }

    fn bit_extract(
        &mut self,
        value: Self::NodeRef,
        start: Self::NodeRef,
        length: Self::NodeRef,
    ) -> Self::NodeRef {
        let typ = value.typ().clone();
        match (value.kind(), start.kind(), length.kind()) {
            // total constant
            (
                NodeKind::Constant { value, .. },
                NodeKind::Constant { value: start, .. },
                NodeKind::Constant { value: length, .. },
            ) => self.constant(
                bit_extract(*value, *start, *length),
                Type::Unsigned(u32::try_from(*length).unwrap()),
            ),
            // register read direct
            (
                NodeKind::GuestRegister { offset },
                NodeKind::Constant {
                    value: bit_extract_offset,
                    ..
                },
                NodeKind::Constant {
                    value: length_value @ (8 | 16 | 32 | 64 | 128),
                    ..
                },
            ) => {
                let length = u32::try_from(*length_value).unwrap();

                let new_typ = match typ {
                    Type::Unsigned(_) => Type::Unsigned(length),
                    _ => todo!("{typ:?}"),
                };

                assert!((bit_extract_offset % 8) == 0);

                self.read_register(*offset + (bit_extract_offset / 8), new_typ)
            }
            // // register read with bitextract
            // (
            //     NodeKind::GuestRegister { offset },
            //     NodeKind::Constant {
            //         value: start_value, ..
            //     },
            //     NodeKind::Constant {
            //         value: length_value,
            //         ..
            //     },
            // ) => {
            //     let start_quotient = start_value / 8;
            //     let start_remainder = start_value % 8;
            //     let length = u16::try_from(*length_value).unwrap();
            //     let new_typ = match typ {
            //         Type::Unsigned(_) => Type::Unsigned(length),
            //         _ => todo!("{typ:?}"),
            //     };
            //     todo!();
            // }

            // concat optimization
            (
                NodeKind::BitInsert {
                    target: bitins_target,
                    source: bitins_source,
                    start: bitins_start,
                    length: bitins_length,
                },
                NodeKind::Constant {
                    value: bitext_start,
                    ..
                },
                NodeKind::Constant {
                    value: bitext_length,
                    ..
                },
            ) => {
                let NodeKind::Constant {
                    value: bitins_start,
                    ..
                } = bitins_start.kind()
                else {
                    panic!()
                };

                let NodeKind::Constant {
                    value: bitins_length,
                    ..
                } = bitins_length.kind()
                else {
                    panic!()
                };

                // extracting exactly what was inserted
                if bitext_start == bitins_start && bitext_length == bitins_length {
                    bitins_source.clone()
                }
                // extracting from the original insert target up to the start of the inserted
                // portion
                else if *bitext_start == 0 && bitext_length == bitins_start {
                    // need to truncate to the currently requested extract length
                    //
                    // truncate should be correct because target should be bigger to hold whatever
                    // was being inserted
                    self.cast(
                        bitins_target.clone(),
                        Type::Unsigned(u32::try_from(*bitext_length).unwrap()),
                        CastOperationKind::Truncate,
                    )
                }
                // overlap, gnarly to deal with :(
                else {
                    // AAAA0000
                    //     ^ start = 16, length = 16
                    // AAAABBBB
                    //   [  ]
                    //
                    // A = bitins_target
                    // B = bitins_source
                    // C = extracted final region

                    // length of the total A region
                    let a_length = *bitins_start;
                    let a_range = 0..a_length;
                    // length of the total B region
                    let b_length = *bitins_length;
                    let b_range = a_length..(a_length + b_length);

                    let c_start = *bitext_start;
                    let c_length = *bitext_length;
                    let c_end = c_start + c_length;

                    log::trace!(
                        "(target[{bitins_start}/{bitins_length}]) = source)[{bitext_start}/{bitext_length}]",
                    );

                    match (
                        a_range.contains(&c_start),
                        a_range.contains(&c_end),
                        b_range.contains(&c_start),
                        b_range.contains(&c_end),
                    ) {
                        (true, true, false, false) => {
                            log::trace!("= target[{c_start}/{c_length}]");
                            let start = self.constant(c_start, Type::Signed(64));
                            let length = self.constant(c_length, Type::Signed(64));
                            self.bit_extract(bitins_target.clone(), start, length)
                        }
                        (false, false, true, true) => {
                            let start = c_start - a_length;
                            log::trace!("= source[{start}/{c_length}]");
                            let start = self.constant(start, Type::Signed(64));
                            let length = self.constant(c_length, Type::Signed(64));
                            self.bit_extract(bitins_source.clone(), start, length)
                        }
                        (true, false, false, true) => {
                            // subregion of A we're extracting
                            let a_subregion_start = *bitext_start;
                            let a_subregion_length = a_length - a_subregion_start;

                            // subregion of B we're extracting
                            let b_subregion_start = 0;
                            let b_subregion_length =
                                bitext_length.saturating_sub(a_subregion_length);

                            assert_eq!(a_subregion_length + b_subregion_length, *bitext_length);

                            log::trace!(
                                "= target[{a_subregion_start}/{a_subregion_length}] ++ source[{b_subregion_start}/{b_subregion_length}]",
                            );

                            let a = {
                                let start = self.constant(a_subregion_start, Type::Signed(64));
                                let length = self.constant(a_subregion_length, Type::Signed(64));
                                self.bit_extract(bitins_target.clone(), start, length)
                            };

                            let b = {
                                let start = self.constant(b_subregion_start, Type::Signed(64));
                                let length = self.constant(b_subregion_length, Type::Signed(64));
                                self.bit_extract(bitins_source.clone(), start, length)
                            };

                            let expanded_a = self.cast(
                                a,
                                Type::Unsigned(u32::try_from(*bitext_length).unwrap()),
                                CastOperationKind::ZeroExtend,
                            );

                            let combined = {
                                let start = self.constant(a_subregion_length, Type::Signed(64));
                                let length = self.constant(b_subregion_length, Type::Signed(64));
                                self.bit_insert(expanded_a, b, start, length)
                            };

                            combined
                        }
                        x => todo!("unreachable I think??? {x:?}"),
                    }
                }
            }

            // mul high, has to be done at lower level because we want to emit specific
            // instructions, so pass it down
            (
                NodeKind::BinaryOperation(BinaryOperationKind::Multiply(_, _)),
                NodeKind::Constant { value: 64, .. },
                NodeKind::Constant { value: 64, .. },
            ) => self.node(X86Node {
                typ,
                kind: NodeKind::BitExtract {
                    value,
                    start,
                    length,
                },
            }),

            // known start and length
            (
                _,
                NodeKind::Constant {
                    value: start_value, ..
                },
                NodeKind::Constant {
                    value: length_value,
                    ..
                },
            ) => {
                let value = match (value.kind(), value.typ()) {
                    (
                        NodeKind::Cast {
                            value: pre_cast_value,
                            kind: CastOperationKind::ZeroExtend,
                        },
                        Type::Unsigned(cast_len),
                    ) => {
                        // if we had enough bits before zero extending, use those
                        if u64::from(pre_cast_value.typ().width()) >= *start_value + *length_value {
                            pre_cast_value.clone()
                        } else {
                            value.clone()
                        }
                    }
                    _ => value.clone(),
                };

                // value >> start && mask(length)
                // should emit fixed shift?
                let shifted =
                    self.shift(value, start.clone(), ShiftOperationKind::LogicalShiftRight);

                let cast = self.cast(
                    shifted,
                    Type::Unsigned(u32::try_from(*length_value).unwrap()),
                    CastOperationKind::Truncate,
                );

                let mask = self.constant(
                    mask(u32::try_from(*length_value).unwrap()),
                    cast.typ().clone(),
                );

                self.binary_operation(BinaryOperationKind::And(cast, mask))
            }
            _ => self.node(X86Node {
                typ,
                kind: NodeKind::BitExtract {
                    value,
                    start,
                    length,
                },
            }),
        }
    }

    fn bit_insert(
        &mut self,
        target: Self::NodeRef,
        source: Self::NodeRef,
        start: Self::NodeRef,
        length: Self::NodeRef,
    ) -> Self::NodeRef {
        let typ = target.typ().clone();
        match (target.kind(), source.kind(), start.kind(), length.kind()) {
            (
                NodeKind::Constant {
                    value: target,
                    width: target_width,
                },
                NodeKind::Constant { value: source, .. },
                NodeKind::Constant { value: start, .. },
                NodeKind::Constant { value: length, .. },
            ) => self.constant(
                bit_insert(*target, *source, *start, *length),
                Type::Unsigned(*target_width),
            ),
            // // constant start and length, and source
            // (
            //     _,
            //     NodeKind::Constant {
            //         value: source_value,
            //         width: source_width,
            //     },
            //     NodeKind::Constant { value: start_c, .. },
            //     NodeKind::Constant {
            //         value: length_c, ..
            //     },
            // ) => {
            //     if u64::from(*source_width) != *length_c {
            //         panic!()
            //     }

            //     if *start_c == 0 {  } else { todo!() }
            // }
            // constant start and length
            (
                _,
                _,
                NodeKind::Constant { value: start_c, .. },
                NodeKind::Constant {
                    value: length_c, ..
                },
            ) => {
                // if we're dealing with 128 bit stuff, and is valid for vector stuff, pass it
                // down to the emitter
                if (*start_c >= 64)
                    || ((*start_c + *length_c) >= 64)
                    || (target.typ().width() == 128)
                {
                    if (target.typ().width() == 128)
                        && !(matches!(length_c, 8 | 16 | 32 | 64) && start_c % length_c == 0)
                    {
                        panic!(
                            "unsupported vector stuff, curious if we ever hit this (we shouldnt)"
                        )
                    }

                    return self.node(X86Node {
                        typ,
                        kind: NodeKind::BitInsert {
                            target,
                            source,
                            start,
                            length,
                        },
                    });
                }

                let length = u32::try_from(*length_c).unwrap();
                let start = u32::try_from(*start_c).unwrap();

                let cleared_target = {
                    let mask = self.constant(
                        (!(mask(length).checked_shl(start).unwrap_or_else(|| {
                            panic!(
                                "overflow in shl with {start:?} {length:?} {:?}",
                                target.typ()
                            )
                        }))) & mask(target.typ().width()),
                        target.typ(),
                    );

                    self.binary_operation(BinaryOperationKind::And(target.clone(), mask))
                };

                let shifted_source = {
                    let cast_source =
                        self.cast(source, target.typ(), CastOperationKind::ZeroExtend);

                    let mask = self.constant(mask(length), cast_source.typ());

                    let masked_source =
                        self.binary_operation(BinaryOperationKind::And(cast_source, mask));

                    let start = self.constant(start.into(), Type::Signed(64));

                    self.shift(masked_source, start, ShiftOperationKind::LogicalShiftLeft)
                };

                self.binary_operation(BinaryOperationKind::Or(cleared_target, shifted_source))
            }
            _ => self.node(X86Node {
                typ,
                kind: NodeKind::BitInsert {
                    target,
                    source,
                    start,
                    length,
                },
            }),
        }
    }

    fn bit_replicate(&mut self, pattern: Self::NodeRef, count: Self::NodeRef) -> Self::NodeRef {
        match (pattern.kind(), count.kind()) {
            (
                NodeKind::Constant {
                    value: pattern,
                    width: pattern_width,
                },
                NodeKind::Constant { value: count, .. },
            ) => {
                let mut dest = *pattern;

                for _ in 1..*count {
                    dest <<= pattern_width;
                    dest |= pattern;
                }

                self.constant(
                    dest,
                    Type::Unsigned(*pattern_width * u32::try_from(*count).unwrap()),
                )
            }
            // todo pattern const non const count -> make all possible values and select?
            // todo pattern non const, const count -> unroll shifts?
            // todo pattern single bit
            // todo: const, partial const
            (_, _) => self.node(X86Node {
                typ: Type::Unsigned(64),
                kind: NodeKind::BitReplicate { pattern, count },
            }),
        }
    }

    fn select(
        &mut self,
        condition: Self::NodeRef,
        true_value: Self::NodeRef,
        false_value: Self::NodeRef,
    ) -> Self::NodeRef {
        match condition.kind() {
            NodeKind::Constant { value, .. } => {
                if *value == 0 {
                    false_value
                } else {
                    true_value
                }
            }
            _ => self.node(X86Node {
                typ: true_value.typ().clone(),
                kind: NodeKind::Select {
                    condition,
                    true_value,
                    false_value,
                },
            }),
        }
    }

    fn write_register(&mut self, offset: u64, value: Self::NodeRef) {
        // todo: validate offset + width is within register file

        // potential issue: read nodes that refer to this regster, which are live past
        // this write how can we detect this?

        // if offset == flags register
        if offset == self.ctx().n_offset
            || offset == self.ctx().z_offset
            || offset == self.ctx().c_offset
            || offset == self.ctx().v_offset
        {
            // look back to see if we're extracting a bit out of get_flags
            if let Some(get_flags_target) = contains_get_flags(&value) {
                assert!(matches!(
                    get_flags_target.kind(),
                    NodeKind::TernaryOperation(TernaryOperationKind::AddWithCarry(_, _, _)),
                ));

                // generates ADC on first time, will be cached on subsequent runs
                let operand = self.to_operand(&get_flags_target);

                if !self.sets_flags.contains_key(&get_flags_target) {
                    let id = self.ctx_mut().allocate_variable_id();
                    self.push_instruction(
                        Instruction::mov(operand, Operand::greg(operand.width(), id)).unwrap(),
                    );
                    self.sets_flags.insert(get_flags_target, id);
                }

                let dest = Operand::mem_base_displ(
                    Width::_8,
                    Register::Physical(PhysicalRegister::RBP),
                    offset.try_into().unwrap(),
                );

                self.push_instruction(if offset == self.ctx().n_offset {
                    Instruction::sets(dest)
                } else if offset == self.ctx().z_offset {
                    Instruction::sete(dest)
                } else if offset == self.ctx().c_offset {
                    Instruction::setc(dest)
                } else if offset == self.ctx().v_offset {
                    Instruction::seto(dest)
                } else {
                    unreachable!()
                });

                return;
            }
        }

        if offset == self.ctx().el_offset {
            let function = self.function_ptr(write_to_el as u64);

            let old = self.read_register(self.ctx().el_offset, Type::Unsigned(64));
            let new = self.cast(
                value.clone(),
                Type::Unsigned(64),
                CastOperationKind::ZeroExtend,
            );

            let mut args = Vec::new_in(self.ctx().allocator());
            args.push(old);
            args.push(new);

            self.call(function, args);
        }

        let value = self.to_operand(&value);
        let width = value.width();

        self.push_instruction(
            Instruction::mov(
                value,
                Operand::mem_base_displ(
                    width,
                    Register::Physical(PhysicalRegister::RBP),
                    offset.try_into().unwrap(),
                ),
            )
            .unwrap(),
        );

        // TODO: Arch-specific hack
        if offset == self.ctx().sctlr_el1_offset
            || offset == self.ctx().ttbr0_el1_offset
            || offset == self.ctx().ttbr1_el1_offset
        {
            // return with invalidate code
            self.execution_result.set_need_tlb_invalidate(true);
        }
    }

    fn read_memory(&mut self, address: Self::NodeRef, typ: Type) -> Self::NodeRef {
        self.node(X86Node {
            typ,
            kind: NodeKind::ReadMemory { address },
        })
    }

    fn write_memory(
        &mut self,
        address: Self::NodeRef,
        value: Self::NodeRef,
        is_unprivileged: bool,
    ) {
        let address = self.to_operand(&address);
        let OperandKind::Register(address_reg) = address.kind() else {
            panic!()
        };

        let value = self.to_operand(&value);
        let width = value.width();

        // It occurs to me that the Arm distribution we're running is a 39-bit address
        // space

        //  Well we've got a fucking 48-bit address space

        //  So we can do high and low in one page table?

        //  So, we can just treat their canonical upper addresses, as access in our
        // canonical lower range

        //  Yes

        //   Just a simple bit of bit shifting and masking should do the trick

        // Amazing

        // wait so the high address I'm seeing in the store instruction is a bug then?

        // So, Arm's address space looks like this:
        // 0000 0000 0000 0000 .. 0000 007F FFFF FFFF
        // FFFF FF80 0000 0000 .. FFFF FFFF FFFF FFFF

        // x86_64 with 48-bit addressing looks like
        // 0000 0000 0000 0000 .. 0000 7FFF FFFF FFFF
        // FFFF 8000 0000 0000 .. FFFF FFFF FFFF FFFF

        // if we mask highest 6 nibbles we get a contiguous address space

        if self.ctx().memory_mask {
            let mask = Operand::vreg(Width::_64, self.next_vreg());
            self.push_instruction(
                Instruction::mov(Operand::imm(Width::_64, 0x0000_00FF_FFFF_FFFF), mask).unwrap(),
            );
            self.push_instruction(Instruction::and(mask, address));
        }

        if is_unprivileged {
            self.push_instruction(
                Instruction::mov(
                    Operand::imm(Width::_32, 1),
                    Operand::mem_seg_displ(32, SegmentRegister::FS, 16),
                )
                .unwrap(),
            );
        };

        self.push_instruction(
            Instruction::mov(value, Operand::mem_base_displ(width, *address_reg, 0)).unwrap(),
        );

        if is_unprivileged {
            self.push_instruction(
                Instruction::mov(
                    Operand::imm(Width::_32, 0),
                    Operand::mem_seg_displ(32, SegmentRegister::FS, 16),
                )
                .unwrap(),
            );
        }
    }

    fn branch(
        &mut self,
        condition: Self::NodeRef,
        true_target: Self::BlockRef,
        false_target: Self::BlockRef,
    ) {
        match condition.kind() {
            NodeKind::Constant { .. } => {
                todo!("this was handled in models.rs")
            }
            NodeKind::BinaryOperation(BinaryOperationKind::CompareEqual(left, right)) => {
                let left = self.to_operand(left);
                let right = self.to_operand(right);

                match (left.kind(), right.kind()) {
                    (OperandKind::Immediate(_), OperandKind::Immediate(_)) => {
                        todo!()
                    }
                    (_, OperandKind::Immediate(0)) => {
                        self.push_instruction(Instruction::test(left, left).unwrap())
                    }
                    (_, OperandKind::Immediate(_)) => {
                        self.push_instruction(Instruction::cmp(right, left))
                    }
                    _ => self.push_instruction(Instruction::cmp(left, right)),
                }

                self.push_instruction(Instruction::jne(false_target));
                self.push_target(false_target);

                self.push_instruction(Instruction::jmp(true_target));
                self.push_target(true_target);
            }
            _ => {
                let condition = self.to_operand(&condition);

                self.push_instruction(Instruction::test(condition, condition).unwrap());

                self.push_instruction(Instruction::jne(true_target));
                self.push_target(true_target);

                self.push_instruction(Instruction::jmp(false_target));
                self.push_target(false_target);
            }
        }
    }

    fn jump(&mut self, target: Self::BlockRef) {
        self.push_instruction(Instruction::jmp(target));
        self.push_target(target);
    }

    fn prologue(&mut self) {}

    fn leave(&mut self) {
        // Read the interrupt pending field of the guest execution context
        self.push_instruction(
            Instruction::mov(
                Operand::mem_seg_displ(
                    32,
                    SegmentRegister::FS,
                    i32::try_from(offset_of!(GuestExecutionContext, interrupt_pending)).unwrap(),
                ),
                Operand::preg(Width::_32, PhysicalRegister::RAX),
            )
            .unwrap(),
        );

        // ASSUMPTION: It will either be zero or one, so move it into bit 2 position.
        self.push_instruction(Instruction::shl(
            Operand::imm(Width::_32, 2),
            Operand::preg(Width::_32, PhysicalRegister::RAX),
        ));

        // If the execution result we're returning is non-zero, then OR it in.
        if self.execution_result.as_u32() != 0 {
            self.push_instruction(Instruction::or(
                Operand::imm(Width::_32, self.execution_result.as_u32() as u64),
                Operand::preg(Width::_32, PhysicalRegister::RAX),
            ));
        }

        // Return
        self.push_instruction(Instruction::ret());
    }

    fn leave_with_cache(&mut self, chain_cache: u64) {
        let return_block = self.ctx_mut().create_block();

        self.push_instruction(
            Instruction::mov(
                Operand::mem_seg_displ(
                    32,
                    SegmentRegister::FS,
                    i32::try_from(offset_of!(GuestExecutionContext, interrupt_pending)).unwrap(),
                ),
                Operand::preg(Width::_32, PhysicalRegister::RAX),
            )
            .unwrap(),
        );

        self.push_instruction(Instruction::shl(
            Operand::imm(Width::_32, 1),
            Operand::preg(Width::_32, PhysicalRegister::RAX),
        ));

        if self.execution_result.as_u32() != 0 {
            self.push_instruction(Instruction::or(
                Operand::imm(Width::_32, self.execution_result.as_u32() as u64),
                Operand::preg(Width::_32, PhysicalRegister::RAX),
            ));
        }

        self.push_instruction(
            Instruction::test(
                Operand::preg(Width::_32, PhysicalRegister::RAX),
                Operand::preg(Width::_32, PhysicalRegister::RAX),
            )
            .unwrap(),
        );
        self.push_instruction(Instruction::jne(return_block));
        self.push_target(return_block);

        let pc_vreg = Operand::vreg(Width::_64, self.next_vreg());
        self.push_instruction(
            Instruction::mov(
                Operand::mem_base_displ(
                    Width::_64,
                    Register::Physical(PhysicalRegister::RBP),
                    self.ctx().pc_offset() as i32,
                ),
                pc_vreg,
            )
            .unwrap(),
        );

        let shifted_pc_vreg = self.next_vreg();
        let shifted_pc_op = Operand::vreg(Width::_64, shifted_pc_vreg);
        self.push_instruction(Instruction::mov(pc_vreg, shifted_pc_op).unwrap());
        self.push_instruction(Instruction::shr(Operand::imm(Width::_8, 2), shifted_pc_op)); // pc must be 4 byte aligned

        assert_eq!(CHAIN_CACHE_ENTRY_COUNT, (1 << 16));
        let masked_vreg = Operand::vreg(Width::_32, self.next_vreg());
        self.push_instruction(
            Instruction::movzx(
                Operand::vreg(Width::_16, shifted_pc_vreg), /* bottom 16 bits = 65536 entries,
                                                             * check */
                masked_vreg,
            )
            .unwrap(),
        );

        self.push_instruction(Instruction::shl(Operand::imm(Width::_64, 4), masked_vreg));

        let tag = Operand::vreg(Width::_64, self.next_vreg());
        let chain_cache_reg = Operand::vreg(Width::_64, self.next_vreg());
        self.push_instruction(
            Instruction::mov(Operand::imm(Width::_64, chain_cache), chain_cache_reg).unwrap(),
        );

        self.push_instruction(
            Instruction::mov(
                Operand::mem_base_idx_scale(
                    Width::_64,
                    chain_cache_reg.as_register().unwrap(),
                    masked_vreg.as_register().unwrap(),
                    MemoryScale::S1,
                ),
                tag,
            )
            .unwrap(),
        );

        self.push_instruction(Instruction::cmp(tag, pc_vreg));
        self.push_instruction(Instruction::jne(return_block));

        // print an A for every chain
        // self.push_instruction(
        //     Instruction::mov(
        //         Operand::imm(Width::_8, 0x41),
        //         Operand::preg(Width::_8, PhysicalRegister::RAX),
        //     )
        //     .unwrap(),
        // );
        // self.push_instruction(Instruction::out(
        //     Operand::imm(Width::_8, 0xE9),
        //     Operand::preg(Width::_8, PhysicalRegister::RAX),
        // ));

        self.push_instruction(Instruction(Opcode::JMP(Operand::mem_base_idx_scale_displ(
            Width::_64,
            chain_cache_reg.as_register().unwrap(),
            masked_vreg.as_register().unwrap(),
            MemoryScale::S1,
            8,
        ))));

        self.set_current_block(return_block);
        self.push_instruction(Instruction::ret());
    }

    fn read_stack_variable(&mut self, id: usize, typ: Type) -> Self::NodeRef {
        let width = typ.width();

        self.node(X86Node {
            typ,
            kind: NodeKind::ReadStackVariable { id, width },
        })
    }

    fn write_stack_variable(&mut self, id: usize, value: Self::NodeRef) {
        log::debug!("writing stack variable {id:#x}: {value:#?}");

        let value = self.to_operand(&value);

        // let mem = Operand::mem_base_displ(
        //     value.width(),
        //     Register::PhysicalRegister(PhysicalRegister::R14),
        //     -(i32::try_from(offset).unwrap()),
        // );

        // self.push_instruction(Instruction::mov(value, mem).unwrap());

        self.push_instruction(Instruction::mov(value, Operand::greg(value.width(), id)).unwrap());
    }

    fn assert(&mut self, condition: Self::NodeRef, meta: u64) {
        match condition.kind() {
            NodeKind::Constant { value, .. } => {
                if *value == 0 {
                    self.panic("constant assert failed");
                }
            }
            _ => {
                let not_condition = self.unary_operation(UnaryOperationKind::Not(condition));
                let op = self.to_operand(&not_condition);

                self.push_instruction(Instruction::test(op, op).unwrap());
                self.push_instruction(
                    Instruction::mov(
                        Operand::imm(Width::_64, meta),
                        Operand::preg(Width::_64, PhysicalRegister::R15),
                    )
                    .unwrap(),
                );
                self.push_instruction(Instruction::jne(self.panic_block.clone()));
            }
        }
    }

    // returns a tuple of (operation_result, flags)
    fn get_flags(&mut self, operation: Self::NodeRef) -> Self::NodeRef {
        self.node(X86Node {
            typ: Type::Unsigned(4),
            kind: NodeKind::GetFlags { operation },
        })
    }

    fn panic(&mut self, msg: &str) {
        let n = self.to_operand(&self.node(X86Node {
            typ: Type::Unsigned(8),
            kind: NodeKind::Constant {
                value: match msg {
                    "undefined terminator" => 0x50,
                    "default terminator" => 0x51,
                    "constant assert failed" => 0x52,
                    "panic block" => 0x53,
                    "match" => 0x54,
                    _ => todo!("{msg}"),
                },
                width: 8,
            },
        }));

        self.push_instruction(Instruction::int(n));
    }

    fn create_tuple(&mut self, values: Vec<Self::NodeRef, A>) -> Self::NodeRef {
        self.node(X86Node {
            typ: Type::Tuple,
            kind: NodeKind::Tuple(values),
        })
    }

    fn access_tuple(&mut self, tuple: Self::NodeRef, index: usize) -> Self::NodeRef {
        let NodeKind::Tuple(values) = tuple.kind() else {
            panic!("accessing non tuple: {:?}", *tuple.0)
        };

        values[index].clone()
    }

    fn size_of(&mut self, value: Self::NodeRef) -> Self::NodeRef {
        match value.typ() {
            Type::Unsigned(w) | Type::Signed(w) | Type::Floating(w) => {
                self.constant(w.into(), Type::Unsigned(16))
            }

            Type::Bits => {
                if let NodeKind::Constant { width, .. } = value.kind() {
                    self.constant(u64::from(*width), Type::Unsigned(16))
                } else {
                    match value.kind() {
                        NodeKind::Cast {
                            value,
                            kind: CastOperationKind::ZeroExtend,
                        } => match value.typ() {
                            Type::Unsigned(w) => self.constant(w.into(), Type::Unsigned(16)),
                            _ => todo!(),
                        },
                        NodeKind::ReadStackVariable { .. } => self.constant(64, Type::Unsigned(16)),
                        _ => todo!("size of {value:#?}"),
                    }
                }
            }
            Type::Int => self.constant(64, Type::Unsigned(16)),
            Type::Tuple => todo!(),
        }
    }

    fn bits_cast(
        &mut self,
        value: Self::NodeRef,
        length: Self::NodeRef,
        _typ: Type,
        kind: CastOperationKind,
    ) -> Self::NodeRef {
        match (value.kind(), length.kind(), kind) {
            (
                NodeKind::Constant {
                    value: value_value,
                    width: value_width,
                },
                NodeKind::Constant {
                    value: length_value,
                    ..
                },
                CastOperationKind::Truncate,
            ) => {
                let target_length = u32::try_from(*length_value).unwrap();

                assert!(target_length <= *value_width);

                let typ = match value.typ() {
                    Type::Unsigned(_) | Type::Bits => Type::Unsigned(target_length),
                    Type::Signed(_) => Type::Signed(target_length),
                    _ => todo!(),
                };

                self.constant(*value_value & mask(target_length), typ)
            }
            (
                NodeKind::Constant {
                    value: value_value,
                    width: value_width,
                },
                NodeKind::Constant {
                    value: length_value,
                    ..
                },
                CastOperationKind::SignExtend,
            ) => {
                let target_length = u32::try_from(*length_value).unwrap();

                assert!(target_length >= *value_width);

                let typ = match value.typ() {
                    Type::Unsigned(_) | Type::Bits => Type::Unsigned(target_length),
                    Type::Signed(_) => Type::Signed(target_length),
                    _ => todo!(),
                };

                let sign_extended =
                    ((*value_value as i64) << (64 - value_width)) >> (64 - value_width);

                self.constant(sign_extended as u64 & mask(target_length), typ)
            }
            (
                NodeKind::Constant {
                    value: value_value,
                    width: value_width,
                },
                NodeKind::Constant {
                    value: length_value,
                    ..
                },
                CastOperationKind::ZeroExtend,
            ) => {
                let target_length = u32::try_from(*length_value).unwrap();

                assert!(target_length >= *value_width);

                let typ = match value.typ() {
                    Type::Unsigned(_) | Type::Bits => Type::Unsigned(target_length),
                    Type::Signed(_) => Type::Signed(target_length),
                    _ => todo!(),
                };

                self.constant(*value_value, typ)
            }
            (
                _,
                NodeKind::Constant {
                    value: length_value,
                    ..
                },
                CastOperationKind::SignExtend,
            ) => self.cast(
                value,
                Type::Signed(u32::try_from(*length_value).unwrap()),
                CastOperationKind::SignExtend,
            ),
            _ => {
                // todo: attach length information
                // todo: fix other cast operation kinds!
                value
            }
        }
    }

    fn call(&mut self, function: Self::NodeRef, arguments: Vec<Self::NodeRef, A>) {
        self.emit_call(function, arguments, false);
    }

    fn call_with_return(
        &mut self,
        function: Self::NodeRef,
        arguments: Vec<Self::NodeRef, A>,
    ) -> Self::NodeRef {
        self.emit_call(function, arguments, true);

        self.node(X86Node {
            typ: Type::Unsigned(64),
            kind: NodeKind::CallReturnValue,
        })
    }
}

fn sign_extend(value: u64, original_width: u32, target_width: u32) -> u64 {
    if value == 0 {
        return 0;
    }

    const CONTAINER_WIDTH: u32 = u64::BITS;

    let original_width = u32::from(original_width);

    let signed_value = value as i64;

    let shifted_left = signed_value
        .checked_shl(CONTAINER_WIDTH - original_width)
        .unwrap_or_else(|| panic!("failed to shift left {value} by 64 - {original_width}"));

    let shifted_right = shifted_left
        .checked_shr(CONTAINER_WIDTH - original_width)
        .unwrap_or_else(|| panic!("failed to shift right {value} by 64 - {target_width}"));

    shifted_right as u64
}

#[ktest]
fn signextend_64() {
    assert_eq!(64, sign_extend(64, 8, 64));
}

#[derive_where(Debug)]
pub struct X86NodeRef<A: Alloc>(pub Rc<X86Node<A>, A>);

impl<A: Alloc> Clone for X86NodeRef<A> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<A: Alloc> Hash for X86NodeRef<A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state);
    }
}

impl<A: Alloc> Eq for X86NodeRef<A> {}

impl<A: Alloc> PartialEq for X86NodeRef<A> {
    fn eq(&self, other: &X86NodeRef<A>) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<A: Alloc> X86NodeRef<A> {
    pub fn kind(&self) -> &NodeKind<A> {
        &self.0.kind
    }

    pub fn typ(&self) -> Type {
        self.0.typ
    }
}

#[derive_where(Debug)]
pub struct X86Node<A: Alloc> {
    pub typ: Type,
    pub kind: NodeKind<A>,
}

#[derive(Clone)]
#[derive_where(Debug, PartialEq, Eq)]
pub enum NodeKind<A: Alloc> {
    Constant {
        value: u64,
        width: u32,
    },
    FunctionPointer(u64),
    GuestRegister {
        offset: u64,
    },
    ReadMemory {
        address: X86NodeRef<A>,
    },
    UnaryOperation(UnaryOperationKind<A>),
    BinaryOperation(BinaryOperationKind<A>),
    TernaryOperation(TernaryOperationKind<A>),
    Cast {
        value: X86NodeRef<A>,
        kind: CastOperationKind,
    },
    Shift {
        value: X86NodeRef<A>,
        amount: X86NodeRef<A>,
        kind: ShiftOperationKind,
    },
    ReadStackVariable {
        // positive offset here (will be subtracted from RSP)
        id: usize,
        width: u32,
    },
    BitExtract {
        value: X86NodeRef<A>,
        start: X86NodeRef<A>,
        length: X86NodeRef<A>,
    },
    BitInsert {
        target: X86NodeRef<A>,
        source: X86NodeRef<A>,
        start: X86NodeRef<A>,
        length: X86NodeRef<A>,
    },
    BitReplicate {
        pattern: X86NodeRef<A>,
        count: X86NodeRef<A>,
    },
    GetFlags {
        operation: X86NodeRef<A>,
    },
    Tuple(Vec<X86NodeRef<A>, A>),
    Select {
        condition: X86NodeRef<A>,
        true_value: X86NodeRef<A>,
        false_value: X86NodeRef<A>,
    },
    CallReturnValue,
}

#[derive(Clone)]
#[derive_where(Debug, PartialEq, Eq)]
pub enum BinaryOperationKind<A: Alloc> {
    Add(X86NodeRef<A>, X86NodeRef<A>),
    Sub(X86NodeRef<A>, X86NodeRef<A>),
    Multiply(X86NodeRef<A>, X86NodeRef<A>),

    Divide(X86NodeRef<A>, X86NodeRef<A>),
    Modulo(X86NodeRef<A>, X86NodeRef<A>),
    And(X86NodeRef<A>, X86NodeRef<A>),
    Or(X86NodeRef<A>, X86NodeRef<A>),
    Xor(X86NodeRef<A>, X86NodeRef<A>),
    PowI(X86NodeRef<A>, X86NodeRef<A>),
    CompareEqual(X86NodeRef<A>, X86NodeRef<A>),
    CompareNotEqual(X86NodeRef<A>, X86NodeRef<A>),
    CompareLessThan(X86NodeRef<A>, X86NodeRef<A>),
    CompareLessThanOrEqual(X86NodeRef<A>, X86NodeRef<A>),
    CompareGreaterThan(X86NodeRef<A>, X86NodeRef<A>),
    CompareGreaterThanOrEqual(X86NodeRef<A>, X86NodeRef<A>),
}

#[derive(Clone)]
#[derive_where(Debug, PartialEq, Eq)]
pub enum UnaryOperationKind<A: Alloc> {
    Not(X86NodeRef<A>),
    Negate(X86NodeRef<A>),
    Complement(X86NodeRef<A>),
    Power2(X86NodeRef<A>),
    Absolute(X86NodeRef<A>),
    Ceil(X86NodeRef<A>),
    Floor(X86NodeRef<A>),
    SquareRoot(X86NodeRef<A>),
}

#[derive(Clone)]
#[derive_where(Debug, PartialEq, Eq)]
pub enum TernaryOperationKind<A: Alloc> {
    AddWithCarry(X86NodeRef<A>, X86NodeRef<A>, X86NodeRef<A>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastOperationKind {
    ZeroExtend,
    SignExtend,
    Truncate,
    Reinterpret,
    Convert,
    Broadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftOperationKind {
    LogicalShiftLeft,
    LogicalShiftRight,
    ArithmeticShiftRight,
    RotateRight,
    RotateLeft,
}

#[derive(Debug, Clone, Copy)]
pub enum X86BlockMark {
    None,
    Temporary,
    Permanent,
}

pub struct X86Block<A: Alloc> {
    instructions: Vec<Instruction<A>, A>,
    next: Vec<Ref<X86Block<A>>, A>,
    linked: bool,
    mark: X86BlockMark,
}

impl<A: Alloc> X86Block<A> {
    pub fn new_in(allocator: A) -> Self {
        Self {
            instructions: Vec::new_in(allocator.clone()),
            next: Vec::new_in(allocator),
            linked: false,
            mark: X86BlockMark::None,
        }
    }

    pub fn set_linked(&mut self) {
        self.linked = true;
    }

    pub fn is_linked(&self) -> bool {
        self.linked
    }

    pub fn set_mark(&mut self, mark: X86BlockMark) {
        self.mark = mark;
    }

    pub fn get_mark(&self) -> X86BlockMark {
        self.mark
    }

    pub fn append(&mut self, instruction: Instruction<A>) {
        self.instructions.push(instruction);
    }

    pub fn allocate_registers<R: RegisterAllocator>(&mut self, allocator: &mut R) {
        allocator.allocate(self.instructions_mut());
    }

    pub fn instructions(&self) -> &[Instruction<A>] {
        &self.instructions
    }

    pub fn instructions_mut(&mut self) -> &mut Vec<Instruction<A>, A> {
        &mut self.instructions
    }

    pub fn next_blocks(&self) -> &[Ref<X86Block<A>>] {
        &self.next
    }

    pub fn clear_next_blocks(&mut self) {
        self.next.clear();
    }

    pub fn push_next(&mut self, target: Ref<X86Block<A>>) {
        self.next.push(target);
        if self.next.len() > 2 {
            panic!(
                "bad, blocks should not have more than 2 real targets (asserts complicate things)"
            )
        }
    }
}

impl<A: Alloc> Debug for X86Block<A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for instr in &self.instructions {
            writeln!(f, "\t{instr}")?;
        }

        Ok(())
    }
}

// #[cfg(test)]
// mod tests {
//     use {
//         super::{bit_extract, bit_insert, ones},
//         proptest::prelude::*,
//     };

//     #[test]
//     fn ones_smoke() {
//         assert_eq!(0, ones(0));
//         assert_eq!(1, ones(1));
//         assert_eq!(0b111, ones(3));
//         assert_eq!(u32::MAX as u64, ones(u32::BITS as u64));
//         assert_eq!(u64::MAX, ones(u64::BITS as u64));
//     }

//     proptest! {
//         #[test]
//         fn ones_extract(start in 0u64..64, length in 0u64..64) {
//             if start + length <= 64 {
//                 // put some ones somewhere
//                 let value = ones(length) << start;
//                 // extract them out
//                 let extracted = bit_extract(value, start, length);

//                 // check it is equal
//                 assert_eq!(extracted, ones(length))
//             }
//         }

//         #[test]
//         fn bit_insert_extract_prop( target: u64,source: u64, start in
// 0u64..64, length in 0u64..64) {             if start + length <= 64 {
//                 // insert source into target
//                 let inserted = bit_insert(target, source, start, length);
//                 // extract it back out
//                 let extracted = bit_extract(inserted, start, length);

//                 // check it is equal
//                 assert_eq!(extracted, source & ((1 << length) - 1))
//             }
//         }
//     }
// }

fn emit_compare<A: Alloc>(
    op: BinaryOperationKind<A>,
    emitter: &mut X86Emitter<A>,
) -> X86NodeRef<A> {
    use BinaryOperationKind::*;

    let (CompareLessThan(left, right)
    | CompareLessThanOrEqual(left, right)
    | CompareGreaterThan(left, right)
    | CompareGreaterThanOrEqual(left, right)) = &op
    else {
        panic!("only greater/less than comparisons should be handled here");
    };

    match (left.kind(), right.kind()) {
        (
            NodeKind::Constant {
                value: left_value, ..
            },
            NodeKind::Constant {
                value: right_value, ..
            },
        ) => {
            let (is_signed, width) = match (left.typ(), right.typ()) {
                (Type::Signed(lw), Type::Signed(rw)) => {
                    assert_eq!(lw, rw);
                    (true, lw)
                }
                (Type::Unsigned(lw), Type::Unsigned(rw)) => {
                    assert_eq!(lw, rw);
                    (true, lw)
                }
                (Type::Int, Type::Signed(64)) => (true, 64),
                (Type::Unsigned(64), Type::Signed(64)) => (true, 64),
                (Type::Signed(64), Type::Unsigned(64)) => (true, 64),
                types => todo!("compare {types:?}"),
            };

            let result = if is_signed {
                match width {
                    64 => {
                        let left = *left_value as i64;
                        let right = *right_value as i64;

                        match &op {
                            CompareLessThan(_, _) => left < right,
                            CompareLessThanOrEqual(_, _) => left <= right,
                            CompareGreaterThan(_, _) => left > right,
                            CompareGreaterThanOrEqual(_, _) => left >= right,
                            _ => panic!(),
                        }
                    }
                    w => todo!("{w:?}"),
                }
            } else {
                match &op {
                    CompareLessThan(_, _) => left_value < right_value,
                    CompareLessThanOrEqual(_, _) => left_value <= right_value,
                    CompareGreaterThan(_, _) => left_value > right_value,
                    CompareGreaterThanOrEqual(_, _) => left_value >= right_value,
                    _ => panic!(),
                }
            };

            emitter.node(X86Node {
                typ: left.typ().clone(),
                kind: NodeKind::Constant {
                    value: result as u64,
                    width: 1,
                },
            })
        }
        // attempt const eval of reals
        (NodeKind::Tuple(left_real), NodeKind::Tuple(right_real)) => {
            if left_real
                .iter()
                .all(|v| matches!(v.kind(), NodeKind::Constant { .. }))
                && right_real
                    .iter()
                    .all(|v| matches!(v.kind(), NodeKind::Constant { .. }))
            {
                match (left_real.clone().as_slice(), right_real.as_slice()) {
                    ([left_num, left_den], [right_num, right_den]) => {
                        let left = emitter.binary_operation(BinaryOperationKind::Multiply(
                            left_num.clone(),
                            right_den.clone(),
                        ));

                        let right = emitter.binary_operation(BinaryOperationKind::Multiply(
                            left_den.clone(),
                            right_num.clone(),
                        ));

                        assert!(matches!(left.kind(), NodeKind::Constant { .. }));
                        assert!(matches!(right.kind(), NodeKind::Constant { .. }));

                        let op = match op {
                            CompareLessThan(_, _) => CompareLessThan(left, right),
                            CompareLessThanOrEqual(_, _) => CompareLessThanOrEqual(left, right),
                            CompareGreaterThan(_, _) => CompareGreaterThan(left, right),
                            CompareGreaterThanOrEqual(_, _) => {
                                CompareGreaterThanOrEqual(left, right)
                            }
                            _ => panic!(),
                        };

                        emit_compare(op, emitter)
                    }
                    _ => panic!(),
                }
            } else {
                let (left, right) = match (left_real.clone().as_slice(), right_real.as_slice()) {
                    ([left_num, left_den], [right_num, right_den]) => (
                        emitter.binary_operation(BinaryOperationKind::Multiply(
                            left_num.clone(),
                            right_den.clone(),
                        )),
                        emitter.binary_operation(BinaryOperationKind::Multiply(
                            left_den.clone(),
                            right_num.clone(),
                        )),
                    ),
                    _ => panic!(),
                };

                let bin_op = match &op {
                    CompareLessThan(_, _) => CompareLessThan(left, right),
                    CompareLessThanOrEqual(_, _) => CompareLessThanOrEqual(left, right),
                    CompareGreaterThan(_, _) => CompareGreaterThan(left, right),
                    CompareGreaterThanOrEqual(_, _) => CompareGreaterThanOrEqual(left, right),
                    _ => panic!(),
                };

                emitter.binary_operation(bin_op)
            }
        }
        _ => {
            // else emit an X86 node
            emitter.node(X86Node {
                typ: Type::Unsigned(1),
                kind: NodeKind::BinaryOperation(op),
            })
        }
    }
}

fn contains_get_flags<A: Alloc>(value: &X86NodeRef<A>) -> Option<X86NodeRef<A>> {
    match value.kind() {
        NodeKind::GetFlags { operation } => Some(operation.clone()),

        NodeKind::Constant { .. }
        | NodeKind::GuestRegister { .. }
        | NodeKind::ReadMemory { .. }
        | NodeKind::ReadStackVariable { .. } => None,

        NodeKind::UnaryOperation(
            UnaryOperationKind::Absolute(value)
            | UnaryOperationKind::Ceil(value)
            | UnaryOperationKind::Complement(value)
            | UnaryOperationKind::Floor(value)
            | UnaryOperationKind::Negate(value)
            | UnaryOperationKind::Not(value)
            | UnaryOperationKind::Power2(value)
            | UnaryOperationKind::SquareRoot(value),
        )
        | NodeKind::Cast { value, .. }
        | NodeKind::Select {
            condition: value, ..
        } => contains_get_flags(value),

        NodeKind::BinaryOperation(
            BinaryOperationKind::Add(a, b)
            | BinaryOperationKind::And(a, b)
            | BinaryOperationKind::CompareEqual(a, b)
            | BinaryOperationKind::CompareGreaterThan(a, b)
            | BinaryOperationKind::CompareGreaterThanOrEqual(a, b)
            | BinaryOperationKind::CompareLessThan(a, b)
            | BinaryOperationKind::CompareLessThanOrEqual(a, b)
            | BinaryOperationKind::CompareNotEqual(a, b)
            | BinaryOperationKind::Divide(a, b)
            | BinaryOperationKind::Modulo(a, b)
            | BinaryOperationKind::Multiply(a, b)
            | BinaryOperationKind::Or(a, b)
            | BinaryOperationKind::PowI(a, b)
            | BinaryOperationKind::Sub(a, b)
            | BinaryOperationKind::Xor(a, b),
        )
        | NodeKind::Shift {
            value: a,
            amount: b,
            ..
        } => contains_get_flags(a).or_else(|| contains_get_flags(b)),

        NodeKind::BitExtract {
            value: a,
            start: _,
            length: _,
        } => contains_get_flags(a),
        // .or_else(|| contains_get_flags(b))
        // .or_else(|| contains_get_flags(c)),
        NodeKind::Tuple(x86_node_refs) => {
            x86_node_refs.iter().filter_map(contains_get_flags).next()
        }

        _ => panic!(),
    }
}

fn contains_addwithcarry<A: Alloc>(value: &X86NodeRef<A>) -> Option<X86NodeRef<A>> {
    match value.kind() {
        NodeKind::TernaryOperation(TernaryOperationKind::AddWithCarry(_, _, _)) => {
            Some(value.clone())
        }

        NodeKind::Constant { .. }
        | NodeKind::GuestRegister { .. }
        | NodeKind::ReadMemory { .. }
        | NodeKind::ReadStackVariable { .. } => None,

        NodeKind::UnaryOperation(
            UnaryOperationKind::Absolute(value)
            | UnaryOperationKind::Ceil(value)
            | UnaryOperationKind::Complement(value)
            | UnaryOperationKind::Floor(value)
            | UnaryOperationKind::Negate(value)
            | UnaryOperationKind::Not(value)
            | UnaryOperationKind::Power2(value)
            | UnaryOperationKind::SquareRoot(value),
        )
        | NodeKind::GetFlags { operation: value }
        | NodeKind::Cast { value, .. }
        | NodeKind::Select {
            condition: value, ..
        } => contains_addwithcarry(value),

        NodeKind::BinaryOperation(
            BinaryOperationKind::Add(a, b)
            | BinaryOperationKind::And(a, b)
            | BinaryOperationKind::CompareEqual(a, b)
            | BinaryOperationKind::CompareGreaterThan(a, b)
            | BinaryOperationKind::CompareGreaterThanOrEqual(a, b)
            | BinaryOperationKind::CompareLessThan(a, b)
            | BinaryOperationKind::CompareLessThanOrEqual(a, b)
            | BinaryOperationKind::CompareNotEqual(a, b)
            | BinaryOperationKind::Divide(a, b)
            | BinaryOperationKind::Modulo(a, b)
            | BinaryOperationKind::Multiply(a, b)
            | BinaryOperationKind::Or(a, b)
            | BinaryOperationKind::PowI(a, b)
            | BinaryOperationKind::Sub(a, b)
            | BinaryOperationKind::Xor(a, b),
        )
        | NodeKind::Shift {
            value: a,
            amount: b,
            ..
        } => contains_addwithcarry(a).or_else(|| contains_addwithcarry(b)),
        NodeKind::BitExtract {
            value: a,
            start: _,
            length: _,
        } => contains_addwithcarry(a),
        NodeKind::BitInsert {
            target,
            source,
            start,
            length,
        } => todo!(),
        NodeKind::BitReplicate { pattern, count } => todo!(),
        NodeKind::CallReturnValue => todo!(),

        NodeKind::Tuple(x86_node_refs) => x86_node_refs
            .iter()
            .filter_map(contains_addwithcarry)
            .next(),

        NodeKind::FunctionPointer(_) => None,
    }
}

fn is_no_op_and(left_type: Type, right_type: Type, right_constant: u64) -> bool {
    if left_type.width() > 128 {
        // we aren't handling >128 bit values correctly anyway so don't even try
        // anything clever
        false
    } else {
        // right hand side is all 1s for the width of the left hand side value
        (right_constant == mask(left_type.width())
                        // and it's a whole power of two (had some weird issues with intermediate width sizes, todo: fixme)
                        && matches!(right_type.width(), 8 | 16 | 32 | 64 | 128))
                        // OR the width is wider than the container for constants, so we assume the high bits would be 1s if they existed
                        // also todo: fixme because this is super hacky
                        || (left_type.width() > 64 && right_constant == u64::MAX)
    }
}
