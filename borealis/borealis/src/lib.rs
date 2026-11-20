//! Sail frontend for GenSim

use common::intern::InternedString;

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

const FN_DENYLIST: &[&str] = &["gic_read_ram", "PhysMemRead", "initialize_registers"];

pub fn fn_is_allowlisted(name: InternedString) -> bool {
    !FN_DENYLIST.contains(&name.as_ref())
}
