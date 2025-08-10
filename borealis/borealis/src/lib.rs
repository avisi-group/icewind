//! Sail frontend for GenSim

use {
    common::{hashmap::HashSet, intern::InternedString},
    deepsize::DeepSizeOf,
    errctx::PathCtx,
    log::info,
    once_cell::sync::Lazy,
    sailrs::{
        bytes,
        jib_ast::{self, Definition, DefinitionAux, Instruction},
        sail_ast::Location,
        types::{ArchivedListVec, ListVec},
    },
    std::{
        fs::File,
        io::{BufRead, BufReader},
        path::Path,
    },
};

pub mod boom;
pub mod example_fns;
pub mod rudder;
pub mod util;

// evaluates assertions and panics as pure, could be bad
const TREAT_PANICS_AS_PURE_DANGEROUS_UNSAFE: bool = true;

/// Deserializes an AST from an archive.
///
/// Internally, deserialization is performed on a new thread with a sufficient
/// stack size to perform the deserialization.
pub fn load_model(path: &Path) -> ListVec<Definition> {
    let file = File::open(path).map_err(PathCtx::f(path)).unwrap();
    let mmap = unsafe { memmap2::Mmap::map(&file) }.unwrap();

    info!("deserializing");

    let archived: &ArchivedListVec<Definition> = unsafe { rkyv::access_unchecked(&mmap) };
    let jib: ListVec<Definition> = rkyv::deserialize::<_, rkyv::rancor::Error>(archived).unwrap();

    info!("JIB size: {:.2}", bytes(jib.deep_size_of()));

    jib
}

/// Calls to these functions will be replaced with units
pub const DELETED_CALLS: &[&str] = &[
    "RestoreTransactionCheckpointParameterised",
    "Z_set",
    "MaybeZeroSVEUppers",
    "ResetSVEState",
];

pub fn fn_is_allowlisted(name: InternedString) -> bool {
    static FN_DENYLIST: Lazy<HashSet<InternedString>> = Lazy::new(|| {
        BufReader::new(File::open("denylist.txt").unwrap())
            .lines()
            .map(|s| InternedString::from(s.unwrap()))
            .collect()
    });

    !FN_DENYLIST.contains(&name)
}

pub fn jib_wip_filter(jib_ast: ListVec<Definition>) -> impl Iterator<Item = jib_ast::Definition> {
    jib_ast.into_iter().map(|d| {
        if let DefinitionAux::Fundef(name, ret, parameters, body) = d.def {
            let new_body = if fn_is_allowlisted(name.as_interned()) {
                body
            } else {
                vec![Instruction {
                    inner: jib_ast::InstructionAux::Undefined(jib_ast::Type::Unit),
                    annot: (0, Location::Unknown),
                }]
                .into()
            };

            Definition {
                def: DefinitionAux::Fundef(name, ret, parameters, new_body),
                annot: d.annot,
            }
        } else {
            d
        }
    })
}
