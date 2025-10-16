//! Rust interface to `Sail` compiler library

use {
    crate::{error::Error, ffi::run_sail, json::ModelConfig, runtime::RT, types::ListVec},
    color_eyre::{Result, eyre::WrapErr},
    errctx::PathCtx,
    log::trace,
    ocaml::FromValue,
    std::{fs::File, io::BufWriter, path::Path},
};

pub mod error;
pub mod ffi;
pub mod jib_ast;
pub mod json;
pub mod num;
pub mod parse_ast;
pub mod runtime;
pub mod sail_ast;
pub mod shared;
pub mod type_check;
pub mod types;

/// Loads Sail files from `sail.json` model configuration.
///
/// Parses supplied Sail files and returns the AST
pub fn load_from_config<P: AsRef<Path>>(
    config_path: P,
) -> Result<ListVec<jib_ast::Definition>, Error> {
    let ModelConfig { files } = ModelConfig::load(config_path.as_ref())?;

    RT.lock().execute(move |rt| {
        trace!("Compiling Sail");
        let jib = unsafe {
            run_sail(
                rt,
                files
                    .into_iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            )
        }??;

        trace!("Parsing JIB AST");
        let jib = ListVec::<jib_ast::Definition>::from_value(jib);

        Ok(jib)
    })?
}

/// Initialize the logger
pub fn init_logger(filters: &str) -> Result<()> {
    let mut builder = pretty_env_logger::formatted_timed_builder();
    builder.parse_filters(filters);
    builder.try_init().wrap_err("Failed to initialise logger")?;
    Ok(())
}

/// Creates the file supplied in `path`.
///
/// If the file at the supplied path already exists it will
/// be overwritten.
pub fn create_file_buffered<P: AsRef<Path>>(path: P) -> Result<BufWriter<File>> {
    File::options()
        .write(true) // we want to write to the file...
        .create(true) // ...creating if it does not exist..
        .truncate(true) // ...and truncate before writing
        .open(path.as_ref())
        .map(BufWriter::new)
        .map_err(PathCtx::f(path))
        .wrap_err("Failed to write to file")
}
