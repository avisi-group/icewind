pub fn mask<I: Into<u32>>(width: I) -> u128 {
    let n = width.into();
    let (res, overflowed) = 1u128.overflowing_shl(n);

    if overflowed {
        if n > u128::BITS {
            log::warn!("overflowed while generating mask of {n} 1s")
        }

        u128::MAX
    } else {
        res - 1
    }
}
