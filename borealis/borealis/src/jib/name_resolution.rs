use {
    common::intern::InternedString,
    isla_lib::{
        bitvector::b64::B64,
        ir::{Def, Exp, Instr, Loc, Name, Symtab, Ty},
    },
};

pub fn resolve_names(defs: Vec<Def<Name, B64>>, symtab: &Symtab) -> Vec<Def<InternedString, B64>> {
    defs.into_iter().map(|d| resolve_def(d, symtab)).collect()
}

fn resolve_def(def: Def<Name, B64>, symtab: &Symtab) -> Def<InternedString, B64> {
    match def {
        Def::Register(name, ty, instrs) => Def::Register(
            resolve_name(name, symtab),
            resolve_type(ty, symtab),
            resolve_instrs(instrs, symtab),
        ),
        Def::Let(items, instrs) => Def::Let(
            items
                .into_iter()
                .map(|(name, ty)| (resolve_name(name, symtab), resolve_type(ty, symtab)))
                .collect(),
            resolve_instrs(instrs, symtab),
        ),
        Def::Enum(name, items) => Def::Enum(
            resolve_name(name, symtab),
            items
                .into_iter()
                .map(|name| resolve_name(name, symtab))
                .collect(),
        ),
        Def::Struct(name, items) => Def::Struct(
            resolve_name(name, symtab),
            items
                .into_iter()
                .map(|(name, ty)| (resolve_name(name, symtab), resolve_type(ty, symtab)))
                .collect(),
        ),
        Def::Union(name, items) => Def::Union(
            resolve_name(name, symtab),
            items
                .into_iter()
                .map(|(name, ty)| (resolve_name(name, symtab), resolve_type(ty, symtab)))
                .collect(),
        ),
        Def::Val(name, types, ty) => Def::Val(
            resolve_name(name, symtab),
            types
                .into_iter()
                .map(|ty| resolve_type(ty, symtab))
                .collect(),
            resolve_type(ty, symtab),
        ),
        Def::Extern(name, a, b, types, ty) => Def::Extern(
            resolve_name(name, symtab),
            a,
            b,
            types
                .into_iter()
                .map(|ty| resolve_type(ty, symtab))
                .collect(),
            resolve_type(ty, symtab),
        ),
        Def::Fn(name, items, instrs) => Def::Fn(
            resolve_name(name, symtab),
            items
                .into_iter()
                .map(|name| resolve_name(name, symtab))
                .collect(),
            resolve_instrs(instrs, symtab),
        ),
        Def::Files(items) => Def::Files(items),
        Def::Pragma(k, v) => Def::Pragma(k, v),
    }
}

fn resolve_name(name: Name, symtab: &Symtab) -> InternedString {
    let demangled = symtab.to_str_demangled(name);

    let demangled = demangled.strip_prefix("z").unwrap_or(demangled).to_owned();

    let demangled = demangled.replace("z3", "#");
    let demangled = demangled.replace("z5", "%");
    let demangled = demangled.replace("zI", "<");
    let demangled = demangled.replace("zK", ">");

    InternedString::from(demangled)
}

fn resolve_type(ty: Ty<Name>, symtab: &Symtab) -> Ty<InternedString> {
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

        Ty::Vector(ty) => Ty::Vector(Box::new(resolve_type(*ty, symtab))),
        Ty::FixedVector(length, ty) => Ty::FixedVector(length, Box::new(resolve_type(*ty, symtab))),
        Ty::List(ty) => Ty::List(Box::new(resolve_type(*ty, symtab))),
        Ty::Ref(ty) => Ty::Ref(Box::new(resolve_type(*ty, symtab))),

        Ty::Enum(name) => Ty::Enum(resolve_name(name, symtab)),
        Ty::Struct(name) => Ty::Struct(resolve_name(name, symtab)),
        Ty::Union(name) => Ty::Union(resolve_name(name, symtab)),
    }
}

fn resolve_instrs(
    instrs: Vec<Instr<Name, B64>>,
    symtab: &Symtab,
) -> Vec<Instr<InternedString, B64>> {
    instrs
        .into_iter()
        .map(|i| resolve_instr(i, symtab))
        .collect()
}

fn resolve_instr(instr: Instr<Name, B64>, symtab: &Symtab) -> Instr<InternedString, B64> {
    match instr {
        Instr::Decl(name, ty, source_loc) => Instr::Decl(
            resolve_name(name, symtab),
            resolve_type(ty, symtab),
            source_loc,
        ),
        Instr::Init(name, ty, exp, source_loc) => Instr::Init(
            resolve_name(name, symtab),
            resolve_type(ty, symtab),
            resolve_expression(exp, symtab),
            source_loc,
        ),
        Instr::Jump(exp, a, source_loc) => {
            Instr::Jump(resolve_expression(exp, symtab), a, source_loc)
        }
        Instr::Goto(a) => Instr::Goto(a),
        Instr::Copy(loc, exp, source_loc) => Instr::Copy(
            resolve_location(loc, symtab),
            resolve_expression(exp, symtab),
            source_loc,
        ),
        Instr::Monomorphize(name, ty, source_loc) => Instr::Monomorphize(
            resolve_name(name, symtab),
            resolve_type(ty, symtab),
            source_loc,
        ),
        Instr::Call(loc, a, b, exps, source_loc) => Instr::Call(
            resolve_location(loc, symtab),
            a,
            resolve_name(b, symtab),
            exps.into_iter()
                .map(|exp| resolve_expression(exp, symtab))
                .collect(),
            source_loc,
        ),
        Instr::PrimopUnary(loc, unary, exp, source_loc) => Instr::PrimopUnary(
            resolve_location(loc, symtab),
            unary,
            resolve_expression(exp, symtab),
            source_loc,
        ),
        Instr::PrimopBinary(loc, binary, exp, exp1, source_loc) => Instr::PrimopBinary(
            resolve_location(loc, symtab),
            binary,
            resolve_expression(exp, symtab),
            resolve_expression(exp1, symtab),
            source_loc,
        ),
        Instr::PrimopVariadic(loc, variadic, exps, source_loc) => Instr::PrimopVariadic(
            resolve_location(loc, symtab),
            variadic,
            exps.into_iter()
                .map(|exp| resolve_expression(exp, symtab))
                .collect(),
            source_loc,
        ),
        Instr::PrimopReset(loc, reset, source_loc) => {
            Instr::PrimopReset(resolve_location(loc, symtab), reset, source_loc)
        }
        Instr::Exit(exit_cause, source_loc) => Instr::Exit(exit_cause, source_loc),
        Instr::Arbitrary => Instr::Arbitrary,
        Instr::End => Instr::End,
    }
}

fn resolve_location(loc: Loc<Name>, symtab: &Symtab) -> Loc<InternedString> {
    match loc {
        Loc::Id(name) => Loc::Id(resolve_name(name, symtab)),
        Loc::Field(loc, name) => Loc::Field(
            Box::new(resolve_location(*loc, symtab)),
            resolve_name(name, symtab),
        ),
        Loc::Addr(loc) => Loc::Addr(Box::new(resolve_location(*loc, symtab))),
    }
}

fn resolve_expression(exp: Exp<Name>, symtab: &Symtab) -> Exp<InternedString> {
    match exp {
        Exp::Id(name) => Exp::Id(resolve_name(name, symtab)),
        Exp::Ref(name) => Exp::Ref(resolve_name(name, symtab)),
        Exp::Bool(b) => Exp::Bool(b),
        Exp::Bits(b64) => Exp::Bits(b64),
        Exp::String(s) => Exp::String(s),
        Exp::Unit => Exp::Unit,
        Exp::I64(i) => Exp::I64(i),
        Exp::I128(i) => Exp::I128(i),
        Exp::Undefined(ty) => Exp::Undefined(resolve_type(ty, symtab)),
        Exp::Struct(name, items) => Exp::Struct(
            resolve_name(name, symtab),
            items
                .into_iter()
                .map(|(name, exp)| (resolve_name(name, symtab), resolve_expression(exp, symtab)))
                .collect(),
        ),
        Exp::Kind(name, exp) => Exp::Kind(
            resolve_name(name, symtab),
            Box::new(resolve_expression(*exp, symtab)),
        ),
        Exp::Unwrap(name, exp) => Exp::Unwrap(
            resolve_name(name, symtab),
            Box::new(resolve_expression(*exp, symtab)),
        ),
        Exp::Field(exp, name) => Exp::Field(
            Box::new(resolve_expression(*exp, symtab)),
            resolve_name(name, symtab),
        ),
        Exp::Call(op, exps) => Exp::Call(
            op,
            exps.into_iter()
                .map(|exp| resolve_expression(exp, symtab))
                .collect(),
        ),
    }
}
