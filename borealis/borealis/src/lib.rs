//! Sail frontend for GenSim

use {
    crate::{
        boom::{
            Ast,
            passes::{
                builtin_fns::HandleBuiltinFunctions, constant_propogation::ConstantPropogation,
                cycle_finder::CycleFinder, destruct_composites::DestructComposites,
                fold_unconditionals::FoldUnconditionals, lower_reals::LowerReals,
                remove_const_branch::RemoveConstBranch, remove_constant_type::RemoveConstantType,
                remove_units::RemoveUnits,
            },
        },
        example_fns::{example_functions, variable_corrupted_example},
        rudder::{
            opt::{self, OptLevel},
            validator,
        },
    },
    common::{hashmap::HashSet, intern::InternedString},
    deepsize::DeepSizeOf,
    errctx::PathCtx,
    log::{debug, info},
    once_cell::sync::Lazy,
    sailrs::{
        bytes, create_file_buffered,
        jib_ast::{self, Definition, DefinitionAux, Instruction},
        sail_ast::Location,
        types::{ArchivedListVec, ListVec},
    },
    std::{
        fs::{File, create_dir_all},
        io::{BufRead, BufReader, Write},
        path::{Path, PathBuf},
    },
};

pub mod boom;
mod example_fns;
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

#[derive(Debug, Clone)]
pub enum GenerationMode {
    CodeGen,
    CodeGenWithIr(PathBuf),
    IrOnly(PathBuf),
}

/// Compiles a Sail model to a Brig module
pub fn sail_to_brig(jib_ast: ListVec<jib_ast::Definition>, path: PathBuf, mode: GenerationMode) {
    let dump_ir = match &mode {
        GenerationMode::CodeGen => None,
        GenerationMode::CodeGenWithIr(p) | GenerationMode::IrOnly(p) => Some(p),
    };

    if let Some(path) = &dump_ir {
        create_dir_all(path).unwrap()
    }

    if let Some(path) = &dump_ir {
        sailrs::jib_ast::pretty_print::print_ast(
            &mut create_file_buffered(path.join("ast.jib")).unwrap(),
            jib_ast.iter(),
        );
    }

    info!("Converting JIB to BOOM");
    let ast = Ast::from_jib(jib_wip_filter(jib_ast));

    // // useful for debugging
    if let Some(path) = &dump_ir {
        boom::pretty_print::print_ast(
            &mut create_file_buffered(path.join("ast.boom")).unwrap(),
            ast.clone(),
        );
    }

    info!("Running passes on BOOM");
    [
        LowerReals::new_boxed(),
        HandleBuiltinFunctions::new_boxed(),
        RemoveConstantType::new_boxed(),
        DestructComposites::new_boxed(),
        RemoveUnits::new_boxed(),
    ]
    .into_iter()
    .for_each(|mut pass| {
        pass.run(ast.clone());
    });
    boom::passes::run_fixed_point(
        ast.clone(),
        &mut [
            FoldUnconditionals::new_boxed(),
            RemoveConstBranch::new_boxed(),
            ConstantPropogation::new_boxed(),
            // MonomorphizeVectors::new_boxed(),
            CycleFinder::new_boxed(),
        ],
    );

    if let Some(path) = &dump_ir {
        boom::pretty_print::print_ast(
            &mut create_file_buffered(path.join("ast.processed.boom")).unwrap(),
            ast.clone(),
        );
    }

    info!("Building rudder");
    let mut rudder = rudder::build::from_boom(&ast.get());

    if let Some(path) = &dump_ir {
        writeln!(
            &mut create_file_buffered(path.join("ast.rudder")).unwrap(),
            "{rudder}"
        )
        .unwrap();
    }

    info!("Validating rudder");
    let msgs = validator::validate(&rudder);
    for msg in msgs {
        debug!("{msg}");
    }

    info!("Optimising rudder");
    opt::optimise(&mut rudder, OptLevel::Level3);

    if let Some(path) = &dump_ir {
        writeln!(
            &mut create_file_buffered(path.join("ast.opt.rudder")).unwrap(),
            "{rudder}"
        )
        .unwrap();
    }

    info!("Validating rudder again");
    let msgs = validator::validate(&rudder);
    for msg in msgs {
        debug!("{msg}");
    }

    rudder
        .functions_mut()
        .extend(example_functions().into_iter());
    let r0_offset = rudder.reg_offset("R0");
    let r1_offset = rudder.reg_offset("R1");
    let r2_offset = rudder.reg_offset("R2");
    rudder
        .functions_mut()
        .extend(variable_corrupted_example(r0_offset, r1_offset, r2_offset).into_iter());

    let to_remove = rudder
        .functions()
        .keys()
        .copied()
        .filter(|name| !fn_is_allowlisted(*name))
        .collect::<Vec<_>>();
    for name in to_remove {
        let function = rudder.functions_mut().get_mut(&name).unwrap();
        let block = function.new_block();
        function.set_entry_block(block);
    }

    {
        let func = rudder
            .functions()
            .get(&InternedString::from_static("AArch64_TakeException"))
            .unwrap();
        rudder::dot::render(
            &mut create_file_buffered(
                dump_ir
                    .unwrap()
                    .join("AArch64_TakeException.rudder.opt.dot"),
            )
            .unwrap(),
            func.arena(),
            func.entry_block(),
        )
        .unwrap();
    }

    if matches!(
        &mode,
        GenerationMode::CodeGen | GenerationMode::CodeGenWithIr(_)
    ) {
        info!("Serializing Rudder");

        let buf = postcard::to_allocvec(&rudder).unwrap();

        info!("Writing {:.2} to {:?}", bytes(buf.len()), &path);
        File::create(path).unwrap().write_all(&buf).unwrap();
    }
}

/// Calls to these functions will be replaced with units
pub const DELETED_CALLS: &[&str] = &[
    "RestoreTransactionCheckpointParameterised",
    "Z_set",
    "MaybeZeroSVEUppers",
    "ResetSVEState",
];

fn fn_is_allowlisted(name: InternedString) -> bool {
    static FN_DENYLIST: Lazy<HashSet<InternedString>> = Lazy::new(|| {
        BufReader::new(File::open("denylist.txt").unwrap())
            .lines()
            .map(|s| InternedString::from(s.unwrap()))
            .collect()
    });

    !FN_DENYLIST.contains(&name)
}

fn jib_wip_filter(jib_ast: ListVec<Definition>) -> impl Iterator<Item = jib_ast::Definition> {
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
