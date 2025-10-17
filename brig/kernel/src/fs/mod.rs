use alloc::{string::String, vec::Vec};

pub mod tar;
pub mod vfs;

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum Error {
    /// File or directory {0:?} was not found
    NotFound(String),
    /// IO error occurred while reading or writing
    #[allow(unused)]
    Io,
}

/// Filesystem
///
/// `'fs` is the lifetime of the filesystem, and is used by implementations of
/// the `File` trait to hold a reference to the parent filesystem.
pub trait Filesystem {
    fn list<S: AsRef<str>>(&mut self, directory: S) -> Result<Vec<String>, Error>;

    fn size<S: AsRef<str>>(&mut self, filename: S) -> Result<usize, Error>;

    fn read_to_vec<S: AsRef<str>>(&mut self, filename: S) -> Result<Vec<u8>, Error>;
}
