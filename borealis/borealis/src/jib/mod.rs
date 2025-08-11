use {
    crate::{fn_is_allowlisted, jib::name_resolution::resolve_names},
    common::intern::InternedString,
    isla_lib::{
        bitvector::b64::B64,
        ir::{Def, Instr, Symtab},
        ir_lexer::new_ir_lexer,
        ir_parser::IrParser,
    },
};

pub mod convert;
pub mod name_resolution;

/// Deserializes an AST from an archive.
///
/// Internally, deserialization is performed on a new thread with a sufficient
/// stack size to perform the deserialization.
pub fn parse_ir<'ir>(ir: &'ir str) -> Vec<Def<InternedString, B64>> {
    let mut symtab = Symtab::new();

    let defs = IrParser::new()
        .parse(&mut symtab, new_ir_lexer(ir))
        .unwrap();

    resolve_names(defs, &symtab)
}

pub fn jib_wip_filter(
    jib_ast: Vec<Def<InternedString, B64>>,
) -> impl Iterator<Item = Def<InternedString, B64>> {
    jib_ast.into_iter().map(|d| {
        if let Def::Fn(name, args, body) = d {
            let new_body = if fn_is_allowlisted(name) {
                body
            } else {
                vec![Instr::Arbitrary].into()
            };

            Def::Fn(name, args, new_body)
        } else {
            d
        }
    })
}
