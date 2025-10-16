use core::num::ParseIntError;

/// Parses `0x`-prefixed, underscore separated hexadecimal values (like a memory
/// address)
///
/// Shouldn't really live here, ideally in some common utility crate, but
/// `plugins_api` is sorta serving that purpose
pub fn parse_hex_prefix<S: AsRef<str>>(s: S) -> Result<u64, ParseIntError> {
    // remove any underscores
    let s = s.as_ref().replace('_', "");
    // remove prefix
    let s = s.trim_start_matches("0x");

    u64::from_str_radix(s, 16)
}

/// Converts any type to a byte slice
///
/// https://stackoverflow.com/a/42186553/8070904
pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    unsafe {
        core::slice::from_raw_parts((p as *const T) as *const u8, ::core::mem::size_of::<T>())
    }
}
