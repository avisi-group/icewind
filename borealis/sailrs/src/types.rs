//! Types and functions for interfacing with OCaml

use {
    common::intern::InternedString,
    deepsize::DeepSizeOf,
    ocaml::{FromValue, Int, ToValue, Value},
    rayon::{iter::IntoParallelIterator, vec::IntoIter},
    std::{collections::LinkedList, ffi::OsStr, fmt::Display, path::PathBuf, slice, vec},
};

/// Kind identifier
///
/// In the Sail AST kind identifiers are strings, but they are invalid Rust
/// `String`s and so are represented here as a `Vec<u8>`.
#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
    DeepSizeOf,
    PartialEq,
    Eq,
)]
pub struct KindIdentifierInner(Vec<u8>);

unsafe impl FromValue for KindIdentifierInner {
    fn from_value(v: Value) -> Self {
        Self(<&[u8]>::from_value(v).to_owned())
    }
}

unsafe impl ToValue for KindIdentifierInner {
    fn to_value(&self, rt: &ocaml::Runtime) -> Value {
        (self.0.as_slice()).to_value(rt)
    }
}

/// Position of a character in a source file
///
/// Can be converted from `Lexing.position` value <https://v2.ocaml.org/api/Lexing.html>.
#[derive(
    Debug,
    Clone,
    FromValue,
    ToValue,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
    DeepSizeOf,
)]
pub struct Position {
    /// File name
    pub pos_fname: InternedString,
    /// Line number
    pub pos_lnum: Int,
    /// Character offset of beginning of line
    pub pos_bol: Int,
    /// Character offset of the position
    pub pos_cnum: Int,
}

impl Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}",
            PathBuf::from(self.pos_fname.to_string())
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("?"),
            self.pos_lnum,
            self.pos_cnum - self.pos_bol
        )
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
    DeepSizeOf,
)]
/// `Vec` wrapper for converting OCaml linked lists into Rust `Vec`.
pub struct ListVec<T>(Vec<T>);

impl<T> ListVec<T> {
    /// Returns an iterator over the inner `Vec`.
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.0.iter()
    }

    /// Returns a new, empty ListVec
    pub fn new() -> Self {
        Self(vec![])
    }
}

impl<T> Default for ListVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> IntoIterator for ListVec<T> {
    type Item = T;

    type IntoIter = vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T: Send> IntoParallelIterator for ListVec<T> {
    type Item = T;
    type Iter = IntoIter<T>;

    fn into_par_iter(self) -> Self::Iter {
        self.0.into_par_iter()
    }
}

impl<T> FromIterator<T> for ListVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self(Vec::from_iter(iter))
    }
}

impl<T> AsRef<[T]> for ListVec<T> {
    fn as_ref(&self) -> &[T] {
        self.0.as_ref()
    }
}

impl<T> From<Vec<T>> for ListVec<T> {
    fn from(value: Vec<T>) -> Self {
        Self(value)
    }
}

unsafe impl<T: FromValue> FromValue for ListVec<T> {
    fn from_value(v: Value) -> Self {
        Self(LinkedList::from_value(v).into_iter().collect())
    }
}

unsafe impl<T: ToValue> ToValue for ListVec<T> {
    fn to_value(&self, rt: &ocaml::Runtime) -> Value {
        self.0.iter().collect::<LinkedList<_>>().to_value(rt)
    }
}
