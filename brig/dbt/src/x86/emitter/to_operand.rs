use {
    crate::{
        emitter::{Emitter, Type},
        x86::{
            emitter::{
                BinaryOperationKind, CastOperationKind, NodeKind, ShiftOperationKind,
                TernaryOperationKind, UnaryOperationKind, X86Emitter, X86NodeRef,
            },
            encoder::{
                Instruction, Operand, OperandKind,
                registers::{PhysicalRegister, Register},
                width::Width,
            },
        },
    },
    common::ktest,
    core::cmp::{Ordering, min},
};

impl<'a, 'ctx> X86Emitter<'ctx> {
    /// Same as `to_operand` but if the value is a constant (of any size), move
    /// it to a register
    pub fn to_operand_reg_promote(&mut self, node: &X86NodeRef) -> Operand {
        if let NodeKind::Constant { .. } | NodeKind::FunctionPointer(_) = node.kind() {
            let width = Width::from_uncanonicalized(node.typ().width()).unwrap();
            let value_reg = Operand::vreg(width, self.next_vreg());
            let value_imm = self.to_operand(node);
            self.push_instruction(Instruction::mov(value_imm, value_reg).unwrap());
            value_reg
        } else {
            self.to_operand(node)
        }
    }

    /// Same as `to_operand_inner` but handles immediate quirks
    pub fn to_operand(&mut self, node: &X86NodeRef) -> Operand {
        let op = self.to_operand_inner(node);

        if let OperandKind::Immediate(value) = op.kind() {
            // todo: optimize for value == 0
            // if value == 0 {
            // }

            // can't move immediates into XMM registers, so promote to reg
            if op.width() > Width::_64 {
                // limit width to 64
                let limited_width = min(Width::_64, op.width());

                let truncated_op = Operand::imm(limited_width, *value);
                let tmp = Operand::vreg(limited_width, self.next_vreg());

                let tmp_full = Operand::vreg(op.width(), self.next_vreg());
                self.push_instruction(Instruction::mov(truncated_op, tmp).unwrap());
                self.push_instruction(Instruction::movzx(tmp, tmp_full).unwrap());

                tmp_full
            } else if *value > u64::try_from(i32::MAX).unwrap() && op.width() > Width::_32 {
                // will fit in a general register, but the immediate is too
                // large

                let low_half = Operand::imm(Width::_64, value & 0x0000_0000_FFFF_FFFF);
                let low_half_reg = Operand::vreg(op.width(), self.next_vreg());
                self.push_instruction(Instruction::mov(low_half, low_half_reg).unwrap());

                let high_half = Operand::imm(Width::_64, (value & 0xFFFF_FFFF_0000_0000) >> 32);
                let high_half_reg = Operand::vreg(op.width(), self.next_vreg());
                self.push_instruction(Instruction::mov(high_half, high_half_reg).unwrap());

                self.push_instruction(Instruction::shl(
                    Operand::imm(Width::_16, 32),
                    high_half_reg,
                ));

                self.push_instruction(Instruction::or(low_half_reg, high_half_reg));

                high_half_reg
            } else {
                op
            }
        } else {
            op
        }
    }

    fn to_operand_inner(&mut self, node: &X86NodeRef) -> Operand {
        if let Some(operand) = self.current_block_operands.get(node) {
            return *operand;
        }

        if let Some(id) = self.sets_flags.get(node) {
            let read = self.read_stack_variable(*id, node.typ());
            return self.to_operand(&read);
        }

        // The node is not cached -- TODO: make sure it wasn't supposed to be emitted
        // before a side-effecty node.

        let op = match node.kind() {
            NodeKind::Constant { value, width } => Operand::imm(
                Width::from_uncanonicalized(*width)
                    .unwrap_or_else(|e| panic!("failed to canonicalize width of {node:?}: {e}")),
                *value,
            ),
            NodeKind::FunctionPointer(target) => Operand::imm(Width::_64, *target),
            NodeKind::CallReturnValue => Operand::preg(Width::_64, PhysicalRegister::RAX),
            NodeKind::GuestRegister { offset } => {
                let width = Width::from_uncanonicalized(node.typ().width()).unwrap_or_else(|e| {
                    panic!("invalid width register at offset {offset:?}: {e:?}")
                });
                let dst = Operand::vreg(width, self.next_vreg());

                self.push_instruction(
                    Instruction::mov(
                        Operand::mem_base_displ(
                            width,
                            Register::Physical(PhysicalRegister::RBP),
                            (*offset).try_into().unwrap(),
                        ),
                        dst,
                    )
                    .unwrap(),
                );

                dst
            }
            NodeKind::ReadStackVariable { id, width } => {
                let width = Width::from_uncanonicalized(*width).unwrap();
                let dst = Operand::vreg(width, self.next_vreg());

                // self.push_instruction(
                //     Instruction::mov(
                //         Operand::mem_base_displ(
                //             width,
                //             Register::PhysicalRegister(PhysicalRegister::R14),
                //             -(i32::try_from(*offset).unwrap()),
                //         ),
                //         dst,
                //     )
                //     .unwrap(),
                // );

                self.push_instruction(Instruction::mov(Operand::greg(width, *id), dst).unwrap());

                dst
            }
            NodeKind::BinaryOperation(kind) => self.binary_operation_to_operand(kind),
            NodeKind::TernaryOperation(kind) => match kind {
                TernaryOperationKind::AddWithCarry(a, b, carry) => {
                    let a_width = Width::from_uncanonicalized(a.typ().width()).unwrap();
                    let b_width = Width::from_uncanonicalized(b.typ().width()).unwrap();

                    assert_eq!(a_width, b_width);
                    assert_eq!(carry.typ().width(), 1);

                    let dst = Operand::vreg(a_width, self.next_vreg());

                    let a = self.to_operand(a);
                    let b = self.to_operand(b);
                    let carry = self.to_operand(carry);
                    self.push_instruction(Instruction::mov(b, dst).unwrap());
                    self.push_instruction(Instruction::adc(a, dst, carry));

                    if self.sets_flags.contains_key(node) {
                        // N
                        self.push_instruction(Instruction::sets(Operand::mem_base_displ(
                            Width::_8,
                            Register::Physical(PhysicalRegister::RBP),
                            i32::try_from(self.ctx().n_offset).unwrap(),
                        )));
                        // Z
                        self.push_instruction(Instruction::sete(Operand::mem_base_displ(
                            Width::_8,
                            Register::Physical(PhysicalRegister::RBP),
                            i32::try_from(self.ctx().z_offset).unwrap(),
                        )));
                        // C
                        self.push_instruction(Instruction::setc(Operand::mem_base_displ(
                            Width::_8,
                            Register::Physical(PhysicalRegister::RBP),
                            i32::try_from(self.ctx().c_offset).unwrap(),
                        )));
                        // V
                        self.push_instruction(Instruction::seto(Operand::mem_base_displ(
                            Width::_8,
                            Register::Physical(PhysicalRegister::RBP),
                            i32::try_from(self.ctx().v_offset).unwrap(),
                        )));
                    }

                    dst
                }
            },
            NodeKind::UnaryOperation(kind) => self.unary_operation_to_operand(kind),
            NodeKind::BitExtract {
                value,
                start,
                length,
            } => {
                if let NodeKind::BinaryOperation(BinaryOperationKind::Multiply(left, right)) =
                    value.kind()
                    && let NodeKind::Constant { value: 64, .. } = start.kind()
                    && let NodeKind::Constant { value: 64, .. } = length.kind()
                {
                    let is_signed = matches!(left.typ(), Type::Signed(_))
                        | matches!(right.typ(), Type::Signed(_));

                    let hi = Operand::preg(Width::_64, PhysicalRegister::RDX);
                    let lo = Operand::preg(Width::_64, PhysicalRegister::RAX);

                    let _0 = Operand::imm(Width::_64, 0);
                    let left = self.to_operand(left);
                    let right = self.to_operand(right);

                    self.push_instruction(Instruction::mov(_0, hi).unwrap());
                    self.push_instruction(Instruction::mov(left, lo).unwrap());

                    if is_signed {
                        self.push_instruction(Instruction::imul1(right, lo, hi));
                    } else {
                        self.push_instruction(Instruction::mul(right, lo, hi));
                    }

                    let result = Operand::vreg(Width::_64, self.next_vreg());

                    self.push_instruction(Instruction::mov(hi, result).unwrap());

                    return result;
                }

                if value.typ().width() > 64 {
                    if let NodeKind::Constant { value: start_c, .. } = start.kind()
                        && let NodeKind::Constant {
                            value: length_c, ..
                        } = length.kind()
                        && matches!(length_c, 8 | 16 | 32 | 64)
                        && start_c % length_c == 0
                    // indexed by the size of the elements, so for a given length, the start must be
                    // a multiple
                    {
                        let src = self.to_operand(value);

                        let index = Operand::imm(Width::_8, start_c / length_c);

                        let dst = Operand::vreg(
                            Width::from_uncanonicalized(*length_c).unwrap(),
                            self.next_vreg(),
                        );

                        self.push_instruction(Instruction::pextr(index, src, dst));

                        dst
                    } else {
                        let out =
                            self.bit_extract_128(value.clone(), start.clone(), length.clone());

                        self.to_operand(&out)
                    }
                } else {
                    let mut value = if let NodeKind::Constant { .. } = value.kind() {
                        let width = Width::from_uncanonicalized(value.typ().width()).unwrap();
                        let value_reg = Operand::vreg(width, self.next_vreg());
                        let value_imm = self.to_operand(value);
                        self.push_instruction(Instruction::mov(value_imm, value_reg).unwrap());
                        value_reg
                    } else {
                        self.to_operand(value)
                    };

                    if value.width() < Width::_64 {
                        let tmp = Operand::vreg(Width::_64, self.next_vreg());
                        self.push_instruction(Instruction::movzx(value, tmp).unwrap());
                        value = tmp;
                    }

                    let start = self.to_operand(start);
                    let length = self.to_operand(length);

                    //  start[0..8] ++ length[0..8];
                    let control_byte = {
                        let mask = Operand::imm(Width::_64, 0xff);

                        let start = {
                            let dst = Operand::vreg(Width::_64, self.next_vreg());
                            self.push_instruction(Instruction::mov(start, dst).unwrap());
                            self.push_instruction(Instruction::and(mask, dst));
                            dst
                        };

                        let length = {
                            let dst = Operand::vreg(Width::_64, self.next_vreg());
                            self.push_instruction(Instruction::mov(length, dst).unwrap());
                            self.push_instruction(Instruction::and(mask, dst));
                            self.push_instruction(Instruction::shl(
                                Operand::imm(Width::_8, 8),
                                dst,
                            ));
                            dst
                        };

                        let dst = Operand::vreg(Width::_64, self.next_vreg());

                        self.push_instruction(Instruction::mov(start, dst).unwrap());
                        self.push_instruction(Instruction::or(length, dst));

                        dst
                    };

                    // todo: this 64 should be the value of `length`
                    let dst = Operand::vreg(Width::_64, self.next_vreg());

                    self.push_instruction(Instruction::bextr(control_byte, value, dst));

                    dst
                }
            }
            NodeKind::Cast { value, kind } => {
                let target_width = Width::from_uncanonicalized(node.typ().width()).unwrap();
                let dst = Operand::vreg(target_width, self.next_vreg());

                let src = self.to_operand(value);

                if src.width() == dst.width() {
                    self.push_instruction(Instruction::mov(src, dst).unwrap());
                } else {
                    use {CastOperationKind::*, Ordering::*};

                    match (kind, src.width().cmp(&dst.width())) {
                        (ZeroExtend, Equal) => {
                            self.push_instruction(Instruction::mov(src, dst).unwrap())
                        }
                        (ZeroExtend, Less) => {
                            self.push_instruction(Instruction::movzx(src, dst).unwrap())
                        }
                        (ZeroExtend, Greater) => {
                            // todo: fix this
                            // panic!(
                            //     "cannot zero extend when src ({src}) is larger than dst
                            // ({dst})\ntarget type: {:?}\n{value:#?}",
                            //     node.typ()
                            // )
                            self.push_instruction(Instruction::mov(src, dst).unwrap())
                        }

                        (SignExtend, Equal) => {
                            self.push_instruction(Instruction::mov(src, dst).unwrap())
                        }
                        (SignExtend, Less) => self.push_instruction(Instruction::movsx(src, dst)),
                        (SignExtend, Greater) => {
                            panic!("cannot zero extend when src ({src}) is larger than dst ({dst})")
                        }

                        (Convert, _) => {
                            panic!("{:?}\n{:#?}", node.typ(), value);
                        }
                        (Truncate, Greater) => {
                            // workaround for XMM truncation, if we change the width to match dst it
                            // will be emitted as a regular register, has to stay 128-bit for it to
                            // be an XMM, but it's fine because there's already a XMM -> GPR mov
                            // that is equivalent to a truncation
                            if src.width() > Width::_64 && dst.width() < Width::_128 {
                                self.push_instruction(Instruction::mov(src, dst).unwrap());
                            } else {
                                // normal case, just access src as a smaller register
                                let mut src = src;
                                src.set_width(dst.width());
                                self.push_instruction(Instruction::mov(src, dst).unwrap());
                            }
                        }
                        (Truncate, Equal | Less) => {
                            panic!(
                                "invalid truncate: source value: {:?}, cast node target type: {:?}, src_width: {}, dst_width: {}",
                                value.typ(),
                                node.typ(),
                                src.width(),
                                dst.width()
                            )
                        }

                        (Reinterpret, _) => match src.width().cmp(&dst.width()) {
                            Ordering::Equal => {
                                self.push_instruction(Instruction::mov(src, dst).unwrap())
                            }
                            Ordering::Less => {
                                self.push_instruction(Instruction::movzx(src, dst).unwrap())
                            }
                            Ordering::Greater => {
                                // same as truncate, todo: actually figure out how/why re-interpret
                                // is different
                                if src.width() > Width::_64 && dst.width() < Width::_128 {
                                    self.push_instruction(Instruction::mov(src, dst).unwrap());
                                } else {
                                    // normal case, just access src as a smaller register
                                    let mut src = src;
                                    src.set_width(dst.width());
                                    self.push_instruction(Instruction::mov(src, dst).unwrap());
                                }
                            }
                        },
                        (Broadcast, _) => todo!(),
                    }
                }

                dst
            }
            NodeKind::Shift {
                value,
                amount,
                kind,
            } => {
                // SPECIAL CASE FOR UMULH BUG
                // getting top 64 bits of multiplication, emit imul and read out RAX
                // todo: tidy
                if let NodeKind::BinaryOperation(BinaryOperationKind::Multiply(a, b)) = value.kind()
                    && let NodeKind::Constant { value: 64, .. } = amount.kind()
                    && *kind == ShiftOperationKind::LogicalShiftRight
                {
                    let dst_hi = Operand::preg(Width::_64, PhysicalRegister::RDX);
                    let dst_lo = Operand::preg(Width::_64, PhysicalRegister::RAX);
                    let src = Operand::vreg(Width::_64, self.next_vreg());

                    let a = self.to_operand(a);
                    let b = self.to_operand(b);
                    self.push_instruction(Instruction::mov(a, src).unwrap());
                    self.push_instruction(Instruction::mov(b, dst_lo).unwrap());
                    self.push_instruction(Instruction::mul(src, dst_lo, dst_hi));

                    // todo: bug will not cache dst_hi
                    return dst_hi;
                }

                let mut amount_op = self.to_operand(amount);
                let value_op = self.to_operand(value);

                if value_op.width() == Width::_128 {
                    match kind {
                        ShiftOperationKind::LogicalShiftRight => {
                            self.shift_right_128(value_op, amount_op)
                        }
                        _ => todo!("{kind:?}"),
                    }
                } else {
                    let dst = Operand::vreg(value_op.width(), self.next_vreg());
                    self.push_instruction(Instruction::mov(value_op, dst).unwrap());

                    if let OperandKind::Register(_) = amount_op.kind() {
                        // truncate (high bits don't matter anyway)
                        amount_op.set_width(Width::_8);
                        let amount_dst = Operand::preg(Width::_8, PhysicalRegister::RCX);
                        self.push_instruction(Instruction::mov(amount_op, amount_dst).unwrap());
                        amount_op = amount_dst;
                    }

                    match kind {
                        ShiftOperationKind::LogicalShiftLeft => {
                            self.push_instruction(Instruction::shl(amount_op, dst));
                        }

                        ShiftOperationKind::LogicalShiftRight => {
                            self.push_instruction(Instruction::shr(amount_op, dst));
                        }

                        ShiftOperationKind::ArithmeticShiftRight => {
                            self.push_instruction(Instruction::sar(amount_op, dst));
                        }
                        _ => todo!("{kind:?}"),
                    }

                    dst
                }
            }

            NodeKind::BitInsert {
                target,
                source,
                start,
                length,
            } => {
                if let NodeKind::Constant { value: start_c, .. } = start.kind()
                    && let NodeKind::Constant {
                        value: length_c, ..
                    } = length.kind() // start and length must be constant
                    && matches!(length_c, 8 | 16 | 32 | 64)
                    && start_c % length_c == 0 // indexed by the size of the elements, so for a given length, the start must be a multiple
                    && target.typ().width() > 64
                // pinsr can only be used on xmm registers
                {
                    let target = self.to_operand(target);

                    let index = Operand::imm(Width::_8, start_c / length_c);

                    // length encoded in source operand width
                    let mut source = self.to_operand_reg_promote(source);
                    source.set_width(Width::from_uncanonicalized(*length_c).unwrap());

                    self.push_instruction(Instruction::pinsr(index, source, target));

                    target
                } else {
                    let out = if target.typ().width() > 64 {
                        self.bit_insert_128(
                            target.clone(),
                            source.clone(),
                            start.clone(),
                            length.clone(),
                        )
                    } else {
                        self.bit_insert_64(
                            target.clone(),
                            source.clone(),
                            start.clone(),
                            length.clone(),
                        )
                    };

                    self.to_operand(&out)
                }
            }

            NodeKind::BitReplicate { pattern, count } => {
                let pattern_width = pattern.typ().width();

                let pattern = self.to_operand(pattern);
                let count = self.to_operand(count);

                let OperandKind::Immediate(count) = *count.kind() else {
                    todo!()
                };

                if count == 0 {
                    panic!()
                }

                let destination_width =
                    Width::from_uncanonicalized(pattern_width * u32::try_from(count).unwrap())
                        .unwrap();

                // zero extend pattern if necessary
                let pattern = if pattern.width() != destination_width {
                    let pattern_zx = Operand::vreg(destination_width, self.next_vreg());
                    self.push_instruction(Instruction::movzx(pattern, pattern_zx).unwrap());
                    pattern_zx
                } else {
                    pattern
                };

                let dest = Operand::vreg(destination_width, self.next_vreg());

                self.push_instruction(Instruction::mov(pattern, dest).unwrap());

                for _ in 1..count {
                    self.push_instruction(Instruction::shl(
                        Operand::imm(Width::_8, u64::from(pattern_width)),
                        dest,
                    ));
                    self.push_instruction(Instruction::or(pattern, dest));
                }

                dest
            }
            NodeKind::GetFlags { .. } => {
                panic!("handled by addwithcarry specialization");
            }
            NodeKind::Tuple(vec) => panic!("cannot convert to operand: {vec:#?}"),
            NodeKind::Select {
                condition,
                true_value,
                false_value,
            } => {
                let width = Width::from_uncanonicalized(true_value.typ().width()).unwrap();
                let dest = Operand::vreg(width, self.next_vreg());

                let condition = self.to_operand(condition);
                let true_value = self.to_operand_reg_promote(true_value);
                let false_value = self.to_operand(false_value);

                assert!(!matches!(condition.kind(), OperandKind::Immediate(_)));

                // if this sequence is modified, the register allocator must be fixed
                self.push_instruction(Instruction::mov(false_value, dest).unwrap());
                self.push_instruction(Instruction::test(condition, condition).unwrap());
                self.push_instruction(Instruction::cmovne(true_value, dest)); // this write to dest does not result in deallocation

                dest
            }
            NodeKind::ReadMemory { address } => {
                let width = Width::from_uncanonicalized(node.typ().width()).unwrap();

                let address = self.to_operand(address);
                let OperandKind::Register(address_reg) = address.kind() else {
                    panic!()
                };

                let dest = Operand::vreg(width, self.next_vreg());

                if self.ctx().memory_mask {
                    let mask = Operand::vreg(Width::_64, self.next_vreg());
                    self.push_instruction(
                        Instruction::mov(Operand::imm(Width::_64, 0x0000_00FF_FFFF_FFFF), mask)
                            .unwrap(),
                    );
                    self.push_instruction(Instruction::and(mask, address));
                }

                self.push_instruction(
                    Instruction::mov(Operand::mem_base_displ(width, *address_reg, 0), dest)
                        .unwrap(),
                );

                dest
            }
        };

        self.current_block_operands.insert(node.clone(), op);
        op
    }

    fn unary_operation_to_operand(&mut self, kind: &UnaryOperationKind) -> Operand {
        match &kind {
            UnaryOperationKind::Complement(value) => {
                let width = Width::from_uncanonicalized(value.typ().width()).unwrap();
                let dst = Operand::vreg(width, self.next_vreg());
                let value = self.to_operand(value);
                self.push_instruction(Instruction::mov(value, dst).unwrap());
                self.push_instruction(Instruction::not(dst));
                dst
            }
            UnaryOperationKind::Not(value) => {
                let width = Width::from_uncanonicalized(value.typ().width()).unwrap();
                let value = self.to_operand(value);
                let dst = Operand::vreg(width, self.next_vreg());

                self.push_instruction(Instruction::cmp(Operand::imm(width, 0), value));
                self.push_instruction(Instruction::sete(dst));
                self.push_instruction(Instruction::and(Operand::imm(width, 1), dst));

                dst
            }
            UnaryOperationKind::Negate(value) => {
                let value = self.to_operand(value);

                self.push_instruction(Instruction::neg(value));

                value
            }
            UnaryOperationKind::Ceil(value) => {
                let NodeKind::Tuple(real) = value.kind() else {
                    panic!();
                };

                let [num, den] = real.as_slice() else {
                    panic!();
                };

                let is_unsigned =
                    matches!(num.typ(), Type::Unsigned(_)) | matches!(den.typ(), Type::Unsigned(_));

                assert_eq!(num.typ().width(), den.typ().width());

                let width = Width::from_uncanonicalized(num.typ().width()).unwrap();

                let divisor = Operand::vreg(width, self.next_vreg());

                if let (NodeKind::Constant { .. }, NodeKind::Constant { .. }) =
                    (num.kind(), den.kind())
                {
                    todo!("const result")
                }

                let num = self.to_operand_reg_promote(num);
                let den = self.to_operand_reg_promote(den);

                let lo = Operand::preg(width, PhysicalRegister::RAX);
                let hi = Operand::preg(width, PhysicalRegister::RDX);

                self.push_instruction(Instruction::mov(num, lo).unwrap());
                self.push_instruction(Instruction::mov(den, divisor).unwrap());

                if !is_unsigned {
                    self.push_instruction(Instruction::cqo());
                    self.push_instruction(Instruction::idiv(divisor));
                } else {
                    self.push_instruction(Instruction::xor(hi, hi));
                    self.push_instruction(Instruction::div(divisor));
                }

                let quotient = Operand::vreg(width, self.next_vreg());

                self.push_instruction(Instruction::mov(lo, quotient).unwrap());

                quotient
            }
            UnaryOperationKind::Floor(value) => {
                let NodeKind::Tuple(real) = value.kind() else {
                    panic!();
                };

                let [num, den] = real.as_slice() else {
                    panic!();
                };

                let is_unsigned =
                    matches!(num.typ(), Type::Unsigned(_)) | matches!(den.typ(), Type::Unsigned(_));

                assert_eq!(num.typ().width(), den.typ().width());

                let width = Width::from_uncanonicalized(num.typ().width()).unwrap();
                let divisor = Operand::vreg(width, self.next_vreg());

                if let (NodeKind::Constant { .. }, NodeKind::Constant { .. }) =
                    (num.kind(), den.kind())
                {
                    todo!("const result")
                }

                let num = self.to_operand_reg_promote(num);
                let den = self.to_operand_reg_promote(den);

                let lo = Operand::preg(width, PhysicalRegister::RAX);
                let hi = Operand::preg(width, PhysicalRegister::RDX);

                self.push_instruction(Instruction::mov(num, lo).unwrap());
                self.push_instruction(Instruction::mov(den, divisor).unwrap());

                if !is_unsigned {
                    self.push_instruction(Instruction::cqo());
                    self.push_instruction(Instruction::idiv(divisor));
                } else {
                    self.push_instruction(Instruction::xor(hi, hi));
                    self.push_instruction(Instruction::div(divisor));
                }

                let quotient = Operand::vreg(width, self.next_vreg());

                self.push_instruction(Instruction::mov(lo, quotient).unwrap());

                quotient
            }
            kind => todo!("{kind:?}"),
        }
    }

    fn binary_operation_to_operand(&mut self, kind: &BinaryOperationKind) -> Operand {
        use BinaryOperationKind::*;

        let (Add(left, right)
        | Sub(left, right)
        | Or(left, right)
        | Modulo(left, right)
        | Divide(left, right)
        | Multiply(left, right)
        | And(left, right)
        | Xor(left, right)
        | PowI(left, right)
        | CompareEqual(left, right)
        | CompareNotEqual(left, right)
        | CompareLessThan(left, right)
        | CompareLessThanOrEqual(left, right)
        | CompareGreaterThan(left, right)
        | CompareGreaterThanOrEqual(left, right)) = kind;

        // do this first to avoid tuple issues
        if let BinaryOperationKind::CompareEqual(left, right)
        | BinaryOperationKind::CompareNotEqual(left, right)
        | BinaryOperationKind::CompareGreaterThan(left, right)
        | BinaryOperationKind::CompareGreaterThanOrEqual(left, right)
        | BinaryOperationKind::CompareLessThan(left, right)
        | BinaryOperationKind::CompareLessThanOrEqual(left, right) = kind
        {
            return encode_compare(kind, self, left.clone(), right.clone());
        }

        // pull out widths but also validate types are compatible
        let (left, right) = match (left.typ(), right.typ()) {
            (Type::Unsigned(_), Type::Unsigned(_)) => {
                let left = self.to_operand(left);
                let right = self.to_operand(right);

                match left.width().cmp(&right.width()) {
                    Ordering::Less => {
                        let tmp = Operand::vreg(right.width(), self.next_vreg());

                        // todo: fix this and also general solution to needing to reg promote 128
                        // bit stuff
                        let left = if tmp.width() > Width::_64
                            && matches!(left.kind(), OperandKind::Immediate(_))
                        {
                            let promoted = Operand::vreg(left.width(), self.next_vreg());
                            self.push_instruction(Instruction::mov(left, promoted).unwrap());
                            promoted
                        } else {
                            left
                        };

                        self.push_instruction(Instruction::movzx(left, tmp).unwrap());
                        (right, tmp)
                    }
                    Ordering::Equal => (left, right),
                    Ordering::Greater => {
                        let tmp = Operand::vreg(left.width(), self.next_vreg());

                        let right = if tmp.width() > Width::_64
                            && matches!(right.kind(), OperandKind::Immediate(_))
                        {
                            let promoted = Operand::vreg(right.width(), self.next_vreg());
                            self.push_instruction(Instruction::mov(right, promoted).unwrap());
                            promoted
                        } else {
                            right
                        };

                        self.push_instruction(Instruction::movzx(right, tmp).unwrap());
                        (left, tmp)
                    }
                }
            }

            (Type::Bits, Type::Unsigned(_)) => {
                let l = self.to_operand(left);
                let r = self.to_operand(right);

                if l.width() == r.width() {
                    (l, r)
                } else {
                    todo!("{left:?} {right:?} => {l:?} {r:?}")
                }
            }
            (Type::Unsigned(_), Type::Bits) => {
                let left = self.to_operand(left);
                let right = self.to_operand(right);

                if left.width() == right.width() {
                    (left, right)
                } else {
                    todo!()
                }
            }
            (Type::Signed(l), Type::Signed(r)) => match l.cmp(&r) {
                Ordering::Less => {
                    let left = self.to_operand(left);
                    let right = self.to_operand(right);
                    let tmp = Operand::vreg(right.width(), self.next_vreg());

                    if left.width() == right.width() {
                        panic!("true widths different but normalized widths equal")
                    }

                    self.push_instruction(Instruction::movsx(left, tmp));
                    (tmp, right)
                }
                Ordering::Equal => (self.to_operand(left), self.to_operand(right)),
                Ordering::Greater => {
                    todo!("sign extend {r} to {l}")
                }
            },

            (Type::Floating(_), Type::Floating(_)) => todo!(),

            (Type::Tuple, Type::Tuple) => {
                todo!()
            }

            (
                Type::Signed(64) | Type::Unsigned(64) | Type::Int,
                Type::Signed(64) | Type::Unsigned(64) | Type::Int,
            ) => (self.to_operand(left), self.to_operand(right)),

            (left, right) => todo!("{left:?} {right:?}"),
        };

        let width = left.width();
        assert_eq!(width, right.width());

        let dst = Operand::vreg(width, self.next_vreg());

        match kind {
            BinaryOperationKind::Add(_, _) => {
                self.push_instruction(Instruction::mov(left, dst).unwrap());
                self.push_instruction(Instruction::add(right, dst));
                dst
            }
            BinaryOperationKind::Sub(_, _) => {
                self.push_instruction(Instruction::mov(left, dst).unwrap());
                self.push_instruction(Instruction::sub(right, dst));
                dst
            }
            BinaryOperationKind::Or(_, _) => {
                self.push_instruction(
                    Instruction::mov(left, dst).unwrap_or_else(|_| panic!("{left} | {right}")),
                );
                self.push_instruction(Instruction::or(right, dst));
                dst
            }

            BinaryOperationKind::Xor(_, _) => {
                self.push_instruction(Instruction::mov(left, dst).unwrap());
                self.push_instruction(Instruction::xor(right, dst));
                dst
            }
            BinaryOperationKind::Multiply(_, _) => {
                self.push_instruction(Instruction::mov(left, dst).unwrap());
                self.push_instruction(Instruction::imul(right, dst));
                dst
            }
            BinaryOperationKind::And(_, _) => {
                self.push_instruction(Instruction::mov(left, dst).unwrap());
                self.push_instruction(Instruction::and(right, dst));

                dst
            }

            BinaryOperationKind::Divide(dividend, divisor) => {
                assert_eq!(dividend.typ().width(), 64);
                assert_eq!(divisor.typ().width(), 64);

                let is_unsigned = matches!(dividend.typ(), Type::Unsigned(_))
                    | matches!(divisor.typ(), Type::Unsigned(_));

                let dividend = self.to_operand(dividend);
                let divisor = self.to_operand_reg_promote(divisor);

                let lo = Operand::preg(width, PhysicalRegister::RAX);
                let hi = Operand::preg(width, PhysicalRegister::RDX);

                self.push_instruction(Instruction::mov(dividend, lo).unwrap());

                if !is_unsigned {
                    self.push_instruction(Instruction::cqo());
                } else {
                    self.push_instruction(Instruction::xor(hi, hi));
                }

                self.push_instruction(Instruction::idiv(divisor));

                let dst = Operand::vreg(width, self.next_vreg());
                self.push_instruction(Instruction::mov(lo, dst).unwrap());

                dst
            }

            BinaryOperationKind::Modulo(dividend, divisor) => {
                assert_eq!(dividend.typ().width(), 64);
                assert_eq!(divisor.typ().width(), 64);

                let is_unsigned = matches!(dividend.typ(), Type::Unsigned(_))
                    | matches!(divisor.typ(), Type::Unsigned(_));

                let dividend = self.to_operand(dividend);
                let divisor = self.to_operand_reg_promote(divisor);

                let hi = Operand::preg(width, PhysicalRegister::RDX);
                let lo = Operand::preg(width, PhysicalRegister::RAX);

                self.push_instruction(Instruction::mov(dividend, lo).unwrap());

                if !is_unsigned {
                    self.push_instruction(Instruction::cqo());
                } else {
                    self.push_instruction(Instruction::xor(hi, hi));
                }

                self.push_instruction(Instruction::idiv(divisor));

                let dst = Operand::vreg(width, self.next_vreg());
                self.push_instruction(Instruction::mov(hi, dst).unwrap());
                dst
            }

            op => todo!("{op:#?}"),
        }
    }

    fn shift_right_128(&mut self, value: Operand, amount: Operand) -> Operand {
        let low_half = Operand::vreg(Width::_64, self.next_vreg());
        self.push_instruction(Instruction::mov(value, low_half).unwrap());

        // pextrq  rdx,  xmm0, 1   # high qword
        let _1 = Operand::imm(Width::_8, 1);
        let high_half = Operand::vreg(Width::_64, self.next_vreg());
        self.push_instruction(Instruction::pextr(_1, value, high_half));

        let amount = if let OperandKind::Register(_) = amount.kind() {
            let cl = Operand::preg(Width::_8, PhysicalRegister::RCX);
            self.push_instruction(Instruction::mov(amount, cl).unwrap());
            cl
        } else {
            amount
        };

        self.push_instruction(Instruction::shrd(amount, high_half, low_half));

        // low half now correct, need to shift high half
        self.push_instruction(Instruction::shr(amount, high_half));

        // recombine
        let result = Operand::vreg(Width::_128, self.next_vreg());
        self.push_instruction(Instruction::movzx(low_half, result).unwrap());

        self.push_instruction(Instruction::pinsr(_1, high_half, result));

        result
    }
}

fn encode_compare(
    kind: &BinaryOperationKind,
    emitter: &mut X86Emitter,
    right: X86NodeRef, /* TODO: this was flipped in order to make tests pass, unflip right
                        * and left and fix the body of the function */
    left: X86NodeRef,
) -> Operand {
    use crate::x86::encoder::OperandKind::*;

    if let (NodeKind::Constant { .. }, NodeKind::Constant { .. })
    | (NodeKind::Tuple(_), NodeKind::Tuple(_)) = (left.kind(), right.kind())
    {
        panic!("should've been fixed earlier")
    }

    let is_signed = match (left.typ(), right.typ()) {
        (Type::Unsigned(_) | Type::Bits | Type::Int, Type::Unsigned(_) | Type::Bits) => false,
        (Type::Signed(_) | Type::Int, Type::Signed(_) | Type::Int) => true,
        _ => panic!("different types in comparison:\n{left:?}\nand\n{right:?}"),
    };

    let left_op = emitter.to_operand(&left);
    let right_op = emitter.to_operand(&right);

    // only valid compare instructions are (source-destination):
    // reg reg
    // reg mem
    // mem reg
    // imm reg
    // imm mem

    // anything else (imm on the right) must be reworked

    match (left_op.kind(), right_op.kind()) {
        (Register(_), Register(_))
        | (Register(_), Memory { .. })
        | (Memory { .. }, Register(_))
        | (Immediate(_), Register(_))
        | (Immediate(_), Memory { .. })
        | (Memory { .. }, Memory { .. }) => {
            let left = if let (Memory { .. }, Memory { .. }) = (left_op.kind(), right_op.kind()) {
                let new_left = Operand::vreg(left_op.width(), emitter.next_vreg());
                emitter.push_instruction(Instruction::mov(left_op, new_left).unwrap());
                new_left
            } else {
                left_op
            };

            emitter.push_instruction(Instruction::cmp(left, right_op));

            // setCC only sets the lowest bit, smallest unit is a byte, so use an 8 bit
            // destination register
            let dst = Operand::vreg(Width::_8, emitter.next_vreg());

            emitter.push_instruction(match (kind, is_signed) {
                (BinaryOperationKind::CompareEqual(_, _), _) => Instruction::sete(dst),
                (BinaryOperationKind::CompareNotEqual(_, _), _) => Instruction::setne(dst),

                (BinaryOperationKind::CompareLessThan(_, _), false) => Instruction::setb(dst),
                (BinaryOperationKind::CompareLessThanOrEqual(_, _), false) => {
                    Instruction::setbe(dst)
                }
                (BinaryOperationKind::CompareGreaterThan(_, _), false) => Instruction::seta(dst),
                (BinaryOperationKind::CompareGreaterThanOrEqual(_, _), false) => {
                    Instruction::setae(dst)
                }

                (BinaryOperationKind::CompareLessThan(_, _), true) => Instruction::setl(dst),
                (BinaryOperationKind::CompareLessThanOrEqual(_, _), true) => {
                    Instruction::setle(dst)
                }
                (BinaryOperationKind::CompareGreaterThan(_, _), true) => Instruction::setg(dst),
                (BinaryOperationKind::CompareGreaterThanOrEqual(_, _), true) => {
                    Instruction::setge(dst)
                }
                _ => todo!("panic!(\"{{kind:?}} is not a compare\")"),
            });

            dst
        }

        (Memory { .. }, Immediate(_)) | (Register(_), Immediate(_)) => {
            emitter.push_instruction(Instruction::cmp(right_op, left_op));
            let dst = Operand::vreg(Width::_8, emitter.next_vreg());

            emitter.push_instruction(match (kind, is_signed) {
                (BinaryOperationKind::CompareEqual(_, _), _) => Instruction::sete(dst),
                (BinaryOperationKind::CompareNotEqual(_, _), _) => Instruction::setne(dst),

                (BinaryOperationKind::CompareLessThan(_, _), false) => Instruction::setae(dst),
                (BinaryOperationKind::CompareLessThanOrEqual(_, _), false) => {
                    Instruction::seta(dst)
                }
                (BinaryOperationKind::CompareGreaterThan(_, _), false) => Instruction::setbe(dst),
                (BinaryOperationKind::CompareGreaterThanOrEqual(_, _), false) => {
                    Instruction::setb(dst)
                }

                (BinaryOperationKind::CompareLessThan(_, _), true) => Instruction::setge(dst),
                (BinaryOperationKind::CompareLessThanOrEqual(_, _), true) => Instruction::setg(dst),
                (BinaryOperationKind::CompareGreaterThan(_, _), true) => Instruction::setle(dst),
                (BinaryOperationKind::CompareGreaterThanOrEqual(_, _), true) => {
                    Instruction::setl(dst)
                }
                _ => todo!(), //panic!("{kind:?} is not a compare"),
            });

            dst
        }

        (Immediate(_), Immediate(_)) => {
            panic!(
                "why was this not const evaluated? {kind:?} {:?} {:?}",
                left, right,
            )
        }
        (Target(_), _) | (_, Target(_)) => panic!("why"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SplitRange {
    low_start: u64,
    low_length: u64,
    high_start: u64,
    high_length: u64,
}

impl SplitRange {
    fn new(start: u64, length: u64) -> Self {
        let low_start = if start >= 64 { 0 } else { start };
        let low_length = if start >= 64 {
            0
        } else {
            let end = min(start + length, 64);

            end - start
        };

        let high_start = if start >= 64 { start - 64 } else { 0 };
        let high_length = length - low_length;

        Self {
            low_start,
            low_length,
            high_start,
            high_length,
        }
    }
}

#[ktest]
fn splitrange_low() {
    assert_eq!(
        SplitRange::new(4, 7),
        SplitRange {
            low_start: 4,
            low_length: 7,
            high_start: 0,
            high_length: 0
        }
    )
}

#[ktest]
fn splitrange_high() {
    assert_eq!(
        SplitRange::new(78, 15),
        SplitRange {
            low_start: 0,
            low_length: 0,
            high_start: 14,
            high_length: 15,
        }
    )
}

#[ktest]
fn splitrange_split_0() {
    assert_eq!(
        SplitRange::new(32, 64),
        SplitRange {
            low_start: 32,
            low_length: 32,
            high_start: 0,
            high_length: 32,
        }
    )
}

#[ktest]
fn splitrange_split_1() {
    assert_eq!(
        SplitRange::new(63, 47),
        SplitRange {
            low_start: 63,
            low_length: 1,
            high_start: 0,
            high_length: 46,
        }
    )
}
