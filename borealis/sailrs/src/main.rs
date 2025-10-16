use {
    clap::Parser,
    color_eyre::Result,
    common::bytes,
    deepsize::DeepSizeOf,
    log::info,
    rkyv::{api::high::to_bytes_with_alloc, ser::allocator::Arena},
    sailrs::{create_file_buffered, init_logger, load_from_config},
    std::{io::Write, path::PathBuf},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Logging filter string (e.g. "borealis=debug" or "trace")
    #[arg(long)]
    log: Option<String>,

    /// Sail model JSON path
    input: PathBuf,

    /// Archive output path
    output: PathBuf,
}

fn main() -> Result<()> {
    color_eyre::install()?;

    // parse command line arguments
    let args = Args::parse();

    // set up the logger, defaulting to no output if the CLI flag was not supplied
    init_logger(args.log.as_deref().unwrap_or("info"))?;

    let jib = load_from_config(args.input)?;

    info!(
        "JIB size: {:.2} bytes, serializing",
        bytes(jib.deep_size_of())
    );

    let mut writer = create_file_buffered(&args.output)?;
    let mut arena = Arena::new();
    let serialized = to_bytes_with_alloc::<_, rkyv::rancor::Error>(&jib, arena.acquire())?;
    writer.write_all(&serialized)?;

    info!("Serialized JIB to {:.2} bytes", bytes(serialized.len()));

    info!("done");

    Ok(())
}
