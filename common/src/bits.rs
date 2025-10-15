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

pub fn bit_insert(target: u64, source: u64, start: u64, length: u64) -> u64 {
    // todo: hack
    if start >= 64 {
        if source == 0 {
            return 0;
        } else {
            panic!("attempting to insert {length} bits of {source} into {target} at {start}");
        }
    }

    let length = u32::try_from(length).unwrap();

    let cleared_target = {
        let mask = !(mask(length)
            .checked_shl(u32::try_from(start).unwrap())
            .unwrap_or_else(|| {
                panic!("overflow in shl with {target:b} {source:?} {start:?} {length:?}")
            }));
        target & mask
    };

    let shifted_source = {
        let mask = mask(length);
        let masked_source = source & mask;
        masked_source << start
    };

    cleared_target | shifted_source
}

pub fn bit_extract(value: u64, start: u64, length: u64) -> u64 {
    (value >> start) & mask(u32::try_from(length).unwrap())
}
