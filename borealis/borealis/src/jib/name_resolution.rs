use {
    common::intern::InternedString,
    isla_lib::{
        bitvector::b64::B64,
        ir::{Def, Exp, Instr, Loc, Name, Symtab, Ty},
    },
};

pub fn resolve_names(defs: Vec<Def<Name, B64>>, symtab: Symtab) -> Vec<Def<InternedString, B64>> {
    let mut state = ResolverState::new(symtab);

    defs.into_iter().map(|d| state.def(d)).collect()
}

struct ResolverState<'ir> {
    symtab: Symtab<'ir>,
}

impl<'ir> ResolverState<'ir> {
    fn new(symtab: Symtab<'ir>) -> Self {
        Self { symtab }
    }

    fn def(&mut self, def: Def<Name, B64>) -> Def<InternedString, B64> {
        match def {
            Def::Register(name, ty, instrs) => {
                Def::Register(self.name(name), self.ty(ty), self.instrs(instrs))
            }
            Def::Let(items, instrs) => Def::Let(
                items
                    .into_iter()
                    .map(|(name, ty)| (self.name(name), self.ty(ty)))
                    .collect(),
                self.instrs(instrs),
            ),
            Def::Enum(name, items) => Def::Enum(
                self.name(name),
                items.into_iter().map(|name| self.name(name)).collect(),
            ),
            Def::Struct(name, items) => Def::Struct(
                self.name(name),
                items
                    .into_iter()
                    .map(|(name, ty)| (self.name(name), self.ty(ty)))
                    .collect(),
            ),
            Def::Union(name, items) => Def::Union(
                self.name(name),
                items
                    .into_iter()
                    .map(|(name, ty)| (self.name(name), self.ty(ty)))
                    .collect(),
            ),
            Def::Val(name, types, ty) => Def::Val(
                self.name(name),
                types.into_iter().map(|ty| self.ty(ty)).collect(),
                self.ty(ty),
            ),
            Def::Extern(name, a, b, types, ty) => Def::Extern(
                self.name(name),
                a,
                b,
                types.into_iter().map(|ty| self.ty(ty)).collect(),
                self.ty(ty),
            ),
            Def::Fn(name, items, instrs) => Def::Fn(
                self.name(name),
                items.into_iter().map(|name| self.name(name)).collect(),
                self.instrs(instrs),
            ),
            Def::Files(items) => Def::Files(items),
            Def::Pragma(k, v) => Def::Pragma(k, v),
        }
    }

    fn name(&self, name: Name) -> InternedString {
        let str = self.symtab.to_str(name);

        let str = str.strip_prefix("z").unwrap_or(str).to_owned();

        let str = str
            .replace("z3", "#")
            .replace("z5", "%")
            .replace("zI", "<")
            .replace("zK", ">")
            .replace("zD", "-");

        InternedString::from(str)
    }

    fn ty(&self, ty: Ty<Name>) -> Ty<InternedString> {
        match ty {
            Ty::I64 => Ty::I64,
            Ty::I128 => Ty::I128,
            Ty::AnyBits => Ty::AnyBits,
            Ty::Unit => Ty::Unit,
            Ty::Bool => Ty::Bool,
            Ty::Bit => Ty::Bit,
            Ty::String => Ty::String,
            Ty::Real => Ty::Real,
            Ty::RoundingMode => Ty::RoundingMode,
            Ty::Bits(width) => Ty::Bits(width),
            Ty::Float(fpty) => Ty::Float(fpty),
            Ty::Vector(ty) => Ty::Vector(Box::new(self.ty(*ty))),
            Ty::FixedVector(length, ty) => Ty::FixedVector(length, Box::new(self.ty(*ty))),
            Ty::List(ty) => Ty::List(Box::new(self.ty(*ty))),
            Ty::Ref(ty) => Ty::Ref(Box::new(self.ty(*ty))),
            Ty::Enum(name) => Ty::Enum(self.name(name)),
            Ty::Struct(name) => Ty::Struct(self.name(name)),
            Ty::Union(name) => Ty::Union(self.name(name)),
        }
    }

    fn instrs(&self, instrs: Vec<Instr<Name, B64>>) -> Vec<Instr<InternedString, B64>> {
        instrs.into_iter().map(|i| self.instr(i)).collect()
    }

    fn instr(&self, instr: Instr<Name, B64>) -> Instr<InternedString, B64> {
        match instr {
            Instr::Decl(name, ty, source_loc) => {
                Instr::Decl(self.name(name), self.ty(ty), source_loc)
            }
            Instr::Init(name, ty, exp, source_loc) => Instr::Init(
                self.name(name),
                self.ty(ty),
                self.expression(exp),
                source_loc,
            ),
            Instr::Jump(exp, a, source_loc) => Instr::Jump(self.expression(exp), a, source_loc),
            Instr::Goto(a) => Instr::Goto(a),
            Instr::Copy(loc, exp, source_loc) => {
                Instr::Copy(self.location(loc), self.expression(exp), source_loc)
            }
            Instr::Monomorphize(name, ty, source_loc) => {
                Instr::Monomorphize(self.name(name), self.ty(ty), source_loc)
            }
            Instr::Call(loc, a, b, exps, source_loc) => Instr::Call(
                self.location(loc),
                a,
                self.name(b),
                exps.into_iter().map(|exp| self.expression(exp)).collect(),
                source_loc,
            ),
            Instr::PrimopUnary(loc, unary, exp, source_loc) => {
                Instr::PrimopUnary(self.location(loc), unary, self.expression(exp), source_loc)
            }
            Instr::PrimopBinary(loc, binary, exp, exp1, source_loc) => Instr::PrimopBinary(
                self.location(loc),
                binary,
                self.expression(exp),
                self.expression(exp1),
                source_loc,
            ),
            Instr::PrimopVariadic(loc, variadic, exps, source_loc) => Instr::PrimopVariadic(
                self.location(loc),
                variadic,
                exps.into_iter().map(|exp| self.expression(exp)).collect(),
                source_loc,
            ),
            Instr::PrimopReset(loc, reset, source_loc) => {
                Instr::PrimopReset(self.location(loc), reset, source_loc)
            }
            Instr::Exit(exit_cause, source_loc) => Instr::Exit(exit_cause, source_loc),
            Instr::Arbitrary => Instr::Arbitrary,
            Instr::End => Instr::End,
        }
    }

    fn location(&self, loc: Loc<Name>) -> Loc<InternedString> {
        match loc {
            Loc::Id(name) => Loc::Id(self.name(name)),
            Loc::Field(loc, name) => Loc::Field(Box::new(self.location(*loc)), self.name(name)),
            Loc::Addr(loc) => Loc::Addr(Box::new(self.location(*loc))),
        }
    }

    fn expression(&self, exp: Exp<Name>) -> Exp<InternedString> {
        match exp {
            Exp::Id(name) => Exp::Id(self.name(name)),
            Exp::Ref(name) => Exp::Ref(self.name(name)),
            Exp::Bool(b) => Exp::Bool(b),
            Exp::Bits(b64) => Exp::Bits(b64),
            Exp::String(s) => Exp::String(s),
            Exp::Unit => Exp::Unit,
            Exp::I64(i) => Exp::I64(i),
            Exp::I128(i) => Exp::I128(i),
            Exp::Undefined(ty) => Exp::Undefined(self.ty(ty)),
            Exp::Struct(name, items) => Exp::Struct(
                self.name(name),
                items
                    .into_iter()
                    .map(|(name, exp)| (self.name(name), self.expression(exp)))
                    .collect(),
            ),
            Exp::Kind(name, exp) => Exp::Kind(self.name(name), Box::new(self.expression(*exp))),
            Exp::Unwrap(name, exp) => Exp::Unwrap(self.name(name), Box::new(self.expression(*exp))),
            Exp::Field(exp, name) => Exp::Field(Box::new(self.expression(*exp)), self.name(name)),
            Exp::Call(op, exps) => Exp::Call(
                op,
                exps.into_iter().map(|exp| self.expression(exp)).collect(),
            ),
        }
    }
}
