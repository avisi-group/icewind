//! Sail frontend for GenSim

use {
    common::{hashmap::HashSet, intern::InternedString},
    once_cell::sync::Lazy,
    std::{
        fs::File,
        io::{BufRead, BufReader},
    },
};

pub mod boom;
pub mod jib;
pub mod jib_legacy;
pub mod rudder;
pub mod util;

// evaluates assertions and panics as pure, could be bad
const TREAT_PANICS_AS_PURE_DANGEROUS_UNSAFE: bool = true;

/// Calls to these functions will be replaced with units
pub const DELETED_CALLS: &[&str] = &[
    "RestoreTransactionCheckpointParameterised",
    "Z_set",
    "MaybeZeroSVEUppers",
    "ResetSVEState",
    "execute_aarch64_instrs_integer_crc",
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
