pub fn mask<I: Into<u32>>(width: I) -> u64 {
    let n = width.into();
    let (res, overflowed) = 1u64.overflowing_shl(n);

    if overflowed {
        if n > u64::BITS {
            log::debug!("overflowed while generating mask of {n} 1s")
        }

        u64::MAX
    } else {
        res - 1
    }
}
