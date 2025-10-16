use {
    borealis::{
        boom, fn_is_allowlisted,
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
    common::bytes,
    errctx::PathCtx,
    log::{debug, info},
    sailrs::{create_file_buffered, init_logger},
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

    if let Some(path) = &args.dump_ir {
        create_dir_all(path).unwrap()
    }

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
            sailrs::jib_ast::pretty_print::print_ast(
                &mut create_file_buffered(path.join("ast.jib")).unwrap(),
                jib_ast.iter(),
            );
        }

        jib_legacy::convert::jib_to_boom(jib_legacy::jib_wip_filter(jib_ast))
    };

    // useful for debugging
    if let Some(path) = &args.dump_ir {
        boom::pretty_print::print_ast(
            &mut create_file_buffered(path.join("ast.boom")).unwrap(),
            ast.clone(),
        );

        // boom::control_flow::dot::render(
        //     &mut create_file_buffered(path.join("
        // decode_aarch32_instrs_UMULL_A1enc_A_txt.dot"))
        //         .unwrap(),
        //     &ast.get()
        //         .functions
        //         .get(&InternedString::from(
        //             "decode_aarch32_instrs_UMULL_A1enc_A_txt",
        //         ))
        //         .unwrap()
        //         .entry_block,
        // )
        // .unwrap();
    }

    info!("Running passes on BOOM");
    boom::passes::run(ast.clone());

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
