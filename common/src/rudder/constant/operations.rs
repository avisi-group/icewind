use {
    crate::rudder::constant::Constant,
    core::{
        cmp::Ordering,
        ops::{Add, BitXor, Div, Mul, Not, Sub},
    },
};

impl Constant {
    // todo: check widths
    pub fn powi(&self, i: Constant) -> Constant {
        let Constant::SignedInteger { value: i_value, .. } = i else {
            panic!();
        };

        match self {
            Constant::FloatingPoint { value, width } => {
                // used as powi not available in `no_std`
                #[allow(unused)]
                use num_traits::float::FloatCore as _;

                let result = value.powi(i32::try_from(i_value).unwrap());

                // some sail source does actually want infinite/NaNs
                // if !result.is_finite() {
                //     panic!("got non-finite result {result} from {f}.powi({i})");
                // }

                Constant::new_float(result, *width)
            }

            _ => todo!(),
        }
    }
}

impl Add for Constant {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (
                Self::UnsignedInteger {
                    value: l,
                    width: l_width,
                },
                Self::UnsignedInteger { value: r, .. },
            ) => Self::new_unsigned(l + r, l_width),
            (
                Self::SignedInteger {
                    value: l,
                    width: l_width,
                },
                Self::UnsignedInteger { value: r, .. },
            ) => Self::new_signed(l + i64::try_from(r).unwrap(), l_width),
            (
                Self::UnsignedInteger {
                    value: l,
                    width: l_width,
                },
                Self::SignedInteger { value: r, .. },
            ) => Self::new_signed(i64::try_from(l).unwrap() + r, l_width),
            (
                Self::SignedInteger {
                    value: l,
                    width: l_width,
                },
                Self::SignedInteger { value: r, .. },
            ) => Self::new_signed(l + r, l_width),
            (
                Self::FloatingPoint {
                    value: l,
                    width: l_width,
                },
                Self::FloatingPoint { value: r, .. },
            ) => Self::new_float(l + r, l_width),
            (l, r) => panic!("invalid types for add: {l:?} {r:?}"),
        }
    }
}

impl Sub for Constant {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (
                Constant::UnsignedInteger {
                    value: l,
                    width: l_width,
                },
                Constant::UnsignedInteger { value: r, .. },
            ) => Constant::new_unsigned(l - r, l_width),
            (
                Constant::SignedInteger {
                    value: l,
                    width: l_width,
                },
                Constant::SignedInteger { value: r, .. },
            ) => Constant::new_signed(l - r, l_width),
            (
                Constant::FloatingPoint {
                    value: l,
                    width: l_width,
                },
                Constant::FloatingPoint { value: r, .. },
            ) => Constant::new_float(l - r, l_width),

            (
                Constant::UnsignedInteger {
                    value: l,
                    width: l_width,
                },
                Constant::SignedInteger {
                    value: r,
                    width: r_width,
                },
            ) => {
                assert_eq!(l_width, r_width);
                Constant::SignedInteger {
                    value: i64::try_from(l).unwrap() - r,
                    width: l_width,
                }
            }
            (l, r) => panic!("invalid types for sub: {l:?} {r:?}"),
        }
    }
}

impl Mul for Constant {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (
                Constant::UnsignedInteger {
                    value: l,
                    width: l_width,
                },
                Constant::UnsignedInteger { value: r, .. },
            ) => Constant::new_unsigned(l * r, l_width),
            (
                Constant::SignedInteger {
                    value: l,
                    width: l_width,
                },
                Constant::SignedInteger { value: r, .. },
            ) => Constant::new_signed(l * r, l_width),
            (
                Constant::SignedInteger {
                    value: l,
                    width: l_width,
                },
                Constant::UnsignedInteger { value: r, .. },
            ) => Constant::new_signed(l * i64::try_from(r).unwrap(), l_width),
            (
                Constant::FloatingPoint {
                    value: l,
                    width: l_width,
                },
                Constant::FloatingPoint { value: r, .. },
            ) => Constant::new_float(l * r, l_width),
            (l, r) => panic!("invalid types for mul: {l:?} {r:?}"),
        }
    }
}

impl Div for Constant {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (
                Constant::UnsignedInteger {
                    value: l,
                    width: l_width,
                },
                Constant::UnsignedInteger { value: r, .. },
            ) => Constant::new_unsigned(l / r, l_width),
            (
                Constant::SignedInteger {
                    value: l,
                    width: l_width,
                },
                Constant::SignedInteger { value: r, .. },
            ) => Constant::new_signed(l / r, l_width),
            (
                Constant::FloatingPoint {
                    value: l,
                    width: l_width,
                },
                Constant::FloatingPoint { value: r, .. },
            ) => Constant::new_float(l / r, l_width),
            (l, r) => panic!("invalid types for div: {l:?} {r:?}"),
        }
    }
}

impl Not for Constant {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Constant::UnsignedInteger { value, width } => Constant::new_unsigned(!value, width),
            Constant::SignedInteger { .. } => todo!("neg??"), /* ConstantValue::SignedInteger(! */
            // v),
            Constant::FloatingPoint { .. }
            | Constant::String(_)
            | Constant::Tuple(_)
            | Constant::Vector(_) => panic!("not a thing"),
        }
    }
}

impl BitXor for Constant {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (
                Constant::UnsignedInteger {
                    value: l,
                    width: l_width,
                },
                Constant::UnsignedInteger { value: r, .. },
            ) => Constant::new_unsigned(l ^ r, l_width),
            (l, r) => panic!("invalid types for xor: {l:?} {r:?}"),
        }
    }
}

impl PartialOrd for Constant {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (
                Constant::UnsignedInteger { value: l, .. },
                Constant::UnsignedInteger { value: r, .. },
            ) => l.partial_cmp(r),
            (
                Constant::SignedInteger { value: l, .. },
                Constant::SignedInteger { value: r, .. },
            ) => l.partial_cmp(r),
            (
                Constant::FloatingPoint { value: l, .. },
                Constant::FloatingPoint { value: r, .. },
            ) => l.partial_cmp(r),

            _ => None,
        }
    }
}
