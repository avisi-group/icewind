use {
    borealis::{GenerationMode, load_model, sail_to_brig},
    clap::Parser,
    color_eyre::eyre::Result,
    log::info,
    sailrs::init_logger,
    std::path::PathBuf,
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

    let jib = load_model(&args.input);

    let mode = if let Some(ir_path) = args.dump_ir {
        std::fs::remove_dir_all(&ir_path).ok();
        if args.ir_only {
            GenerationMode::IrOnly(ir_path)
        } else {
            GenerationMode::CodeGenWithIr(ir_path)
        }
    } else {
        GenerationMode::CodeGen
    };

    sail_to_brig(jib, args.output, mode);

    info!("done");

    Ok(())
}
