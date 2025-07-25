pub fn signed_smallest_width_of_value(value: i128) -> u32 {
    let value = if value.is_negative() {
        value.abs() - 1
    } else {
        value.abs()
    };

    unsigned_smallest_width_of_value(u128::try_from(value).unwrap()) + 1 // +1 becuase it's a signed value
}

pub fn unsigned_smallest_width_of_value(value: u128) -> u32 {
    u32::try_from((value + 1).next_power_of_two().ilog2()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::{signed_smallest_width_of_value, unsigned_smallest_width_of_value};

    #[test]
    fn _0s() {
        assert_eq!(signed_smallest_width_of_value(0), 1);
    }

    #[test]
    fn _0u() {
        assert_eq!(unsigned_smallest_width_of_value(0), 0);
    }

    #[test]
    fn _neg1() {
        assert_eq!(signed_smallest_width_of_value(-1), 1);
    }

    #[test]
    fn _neg2() {
        assert_eq!(signed_smallest_width_of_value(-2), 2);
    }

    #[test]
    fn _5() {
        assert_eq!(unsigned_smallest_width_of_value(5), 3);
    }

    #[test]
    fn _8() {
        assert_eq!(unsigned_smallest_width_of_value(8), 4);
    }

    #[test]
    fn _32s() {
        assert_eq!(signed_smallest_width_of_value(32), 7);
    }
    #[test]
    fn u32max() {
        assert_eq!(unsigned_smallest_width_of_value(u32::MAX as u64), 32);
    }

    #[test]
    fn i32max() {
        assert_eq!(signed_smallest_width_of_value(i32::MAX as i64), 32);
    }

    #[test]
    fn i32min() {
        assert_eq!(signed_smallest_width_of_value(i32::MIN as i64), 32);
    }
}
