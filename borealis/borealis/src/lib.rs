//! Sail frontend for GenSim

use {
    crate::{
        boom::{
            passes::{
                cycle_finder::CycleFinder, fix_exceptions::FixExceptions,
                fold_unconditionals::FoldUnconditionals, monomorphize_vectors::MonomorphizeVectors,
                remove_const_branch::RemoveConstBranch, resolve_return_assigns::ResolveReturns,
            },
            Ast,
        },
        codegen::{codegen_workspace, workspace::write_workspace},
    },
    common::{
        bytes, create_file_buffered,
        intern::{init_interner, interner},
        HashMap,
    },
    deepsize::DeepSizeOf,
    errctx::PathCtx,
    log::{info, trace, warn},
    rkyv::Deserialize,
    sailrs::{
        jib_ast::{self, Definition},
        types::ListVec,
    },
    std::{
        fs::{create_dir_all, File},
        io::Write,
        path::{Path, PathBuf},
    },
};

pub mod boom;
pub mod codegen;
pub mod rudder;

/// Deserializes an AST from an archive.
///
/// Internally, deserialization is performed on a new thread with a sufficient
/// stack size to perform the deserialization.
pub fn load_model(path: &Path) -> ListVec<Definition> {
    let file = File::open(path).map_err(PathCtx::f(path)).unwrap();
    let mmap = unsafe { memmap2::Mmap::map(&file) }.unwrap();

    trace!("deserializing");

    let (jib, strs): (ListVec<Definition>, _) =
        unsafe { rkyv::archived_root::<(ListVec<Definition>, HashMap<String, u32>)>(&mmap) }
            .deserialize(&mut rkyv::Infallible)
            .unwrap();

    trace!("initializing interner");

    init_interner(&strs);

    trace!("JIB size: {:.2}", bytes(jib.deep_size_of()));
    trace!(
        "INTERNER size: {:.2}, {} strings",
        bytes(interner().current_memory_usage()),
        interner().len()
    );

    jib
}

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
    let ast = Ast::from_jib(jib_ast);

    // // useful for debugging
    if let Some(path) = &dump_ir {
        boom::pretty_print::print_ast(
            &mut create_file_buffered(path.join("ast.boom")).unwrap(),
            ast.clone(),
        );
    }

    info!("Running passes on BOOM");
    boom::passes::run_fixed_point(
        ast.clone(),
        &mut [
            FoldUnconditionals::new_boxed(),
            RemoveConstBranch::new_boxed(),
            ResolveReturns::new_boxed(),
            FixExceptions::new_boxed(),
            MonomorphizeVectors::new_boxed(),
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
    let msgs = rudder.validate();
    for msg in msgs {
        warn!("{msg}");
    }

    info!("Optimising rudder");
    rudder.optimise();

    if let Some(path) = &dump_ir {
        writeln!(
            &mut create_file_buffered(path.join("ast.opt.rudder")).unwrap(),
            "{rudder}"
        )
        .unwrap();
    }

    info!("Validating rudder again");
    let msgs = rudder.validate();
    for msg in msgs {
        warn!("{msg}");
    }

    if matches!(
        &mode,
        GenerationMode::CodeGen | GenerationMode::CodeGenWithIr(_)
    ) {
        info!("Generating Rust");
        let ws = codegen_workspace(&rudder);

        info!("Writing workspace to {:?}", &path);
        write_workspace(ws, path);
    }
}
