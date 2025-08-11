use {
    borealis::{
        boom::{
            self,
            passes::{
                builtin_fns::HandleBuiltinFunctions, constant_propogation::ConstantPropogation,
                cycle_finder::CycleFinder, destruct_composites::DestructComposites,
                fold_unconditionals::FoldUnconditionals, lower_reals::LowerReals,
                remove_const_branch::RemoveConstBranch, remove_constant_type::RemoveConstantType,
                remove_units::RemoveUnits,
            },
        },
        fn_is_allowlisted,
        jib::{self, parse_ir},
        jib_legacy::{self, load_model},
        rudder::{
            self,
            example_fns::{example_functions, variable_corrupted_example},
            opt::OptLevel,
            validator,
        },
    },
    clap::Parser,
    color_eyre::eyre::Result,
    errctx::PathCtx,
    log::{debug, info},
    sailrs::{bytes, create_file_buffered, init_logger},
    std::{
        fs::{self, File, create_dir_all},
        io::Write as _,
        path::PathBuf,
    },
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Logging filter string (e.g. "borealis=debug" or "trace")
    #[arg(long)]
    log: Option<String>,

    /// Writes all intermediate representations to disk in the specified folder
    #[arg(long)]
    dump_ir: Option<PathBuf>,

    /// Only generate IR - don't do codegen
    #[arg(long)]
    ir_only: bool,

    /// Use ISLA lib to load JIB
    #[arg(long)]
    isla: bool,

    /// Path to Sail model archive
    input: PathBuf,
    /// Path to brig Rust file
    output: PathBuf,
}

fn main() -> Result<()> {
    color_eyre::install()?;

    // parse command line arguments
    let args = Args::parse();

    // set up the logger, defaulting to no output if the CLI flag was not supplied
    init_logger(args.log.as_deref().unwrap_or("info")).unwrap();

    info!("Converting JIB to BOOM");
    let ast = if args.isla {
        let contents = fs::read_to_string(&args.input)
            .map_err(PathCtx::f(args.input))
            .unwrap();

        let jib_ast = parse_ir(&contents);

        jib::convert::jib_to_boom(jib::jib_wip_filter(jib_ast))
    } else {
        let jib_ast = load_model(&args.input);

        if let Some(path) = &args.dump_ir {
            create_dir_all(path).unwrap()
        }

        if let Some(path) = &args.dump_ir {
            sailrs::jib_ast::pretty_print::print_ast(
                &mut create_file_buffered(path.join("ast.jib")).unwrap(),
                jib_ast.iter(),
            );
        }

        jib_legacy::convert::jib_to_boom(jib_legacy::jib_wip_filter(jib_ast))
    };

    // // useful for debugging
    if let Some(path) = &args.dump_ir {
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

    if let Some(path) = &args.dump_ir {
        boom::pretty_print::print_ast(
            &mut create_file_buffered(path.join("ast.processed.boom")).unwrap(),
            ast.clone(),
        );
    }

    info!("Building rudder");
    let mut rudder = rudder::build::from_boom(&ast.get());

    if let Some(path) = &args.dump_ir {
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
    rudder::opt::optimise(&mut rudder, OptLevel::Level3);

    if let Some(path) = &args.dump_ir {
        writeln!(
            &mut create_file_buffered(path.join("ast.opt.rudder")).unwrap(),
            "{rudder}"
        )
        .unwrap();
    }

    info!("Validating rudder again");
    let msgs = rudder::validator::validate(&rudder);
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

    // {
    //     let func = rudder
    //         .functions()
    //         .get(&InternedString::from_static("AArch64_TakeException"))
    //         .unwrap();
    //     rudder::dot::render(
    //         &mut create_file_buffered(
    //             args.dump_ir
    //                 .unwrap()
    //                 .join("AArch64_TakeException.rudder.opt.dot"),
    //         )
    //         .unwrap(),
    //         func.arena(),
    //         func.entry_block(),
    //     )
    //     .unwrap();
    // }

    info!("Serializing Rudder");
    let buf = postcard::to_allocvec(&rudder).unwrap();

    info!("Writing {:.2} to {:?}", bytes(buf.len()), &args.output);
    File::create(args.output).unwrap().write_all(&buf).unwrap();

    info!("done");

    Ok(())
}
