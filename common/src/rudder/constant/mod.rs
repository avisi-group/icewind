use {
    crate::{
        intern::InternedString,
        mask::mask,
        rudder::types::{PrimitiveType, Type},
    },
    alloc::{boxed::Box, vec::Vec},
    core::fmt::{self, Display, Formatter},
};

mod operations;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Constant {
    UnsignedInteger { value: u64, width: u32 },
    SignedInteger { value: i64, width: u32 },
    FloatingPoint { value: f64, width: u32 },
    String(InternedString),
    Tuple(Vec<Constant>),
    Vector(Vec<Constant>),
}

impl Constant {
    pub fn new_float(value: f64, width: u32) -> Self {
        match width {
            64 => Self::FloatingPoint { value, width },
            _ => todo!("{width}"),
        }
    }

    pub fn new_signed(value: i64, width: u32) -> Self {
        let masked = (value as u64) & mask(width);

        let shift_amount = (i64::BITS as u32) - width;

        let sign_extended = ((masked as i64) << shift_amount) >> shift_amount;

        Self::SignedInteger {
            value: sign_extended,
            width,
        }
    }

    pub fn new_unsigned(value: u64, width: u32) -> Self {
        Self::UnsignedInteger {
            value: value & mask(width),
            width,
        }
    }

    pub fn typ(&self) -> Type {
        match self {
            Self::UnsignedInteger { width, .. } => {
                Type::Primitive(PrimitiveType::UnsignedInteger(*width))
            }
            Self::SignedInteger { width, .. } => {
                Type::Primitive(PrimitiveType::SignedInteger(*width))
            }
            Self::FloatingPoint { width, .. } => {
                Type::Primitive(PrimitiveType::FloatingPoint(*width))
            }
            Self::String(_) => Type::String,
            Self::Vector(vec) => Type::Vector {
                element_count: vec.len(),
                element_type: Box::new(vec[0].typ()),
            },
            Self::Tuple(tuple) => Type::Tuple(tuple.iter().map(Self::typ).collect()),
        }
    }

    pub fn is_zero(&self) -> Option<bool> {
        match self {
            Constant::UnsignedInteger { value, .. } => Some(*value == 0),
            Constant::SignedInteger { value, .. } => Some(*value == 0),
            Constant::FloatingPoint { value, .. } => Some(*value == 0.),

            Constant::String(_) | Constant::Tuple(_) | Constant::Vector(_) => None,
        }
    }

    pub fn smallest_width(&self) -> usize {
        match self {
            Constant::UnsignedInteger { value, .. } => {
                (usize::BITS - value.leading_zeros()) as usize
            }
            Constant::SignedInteger { value, .. } => (isize::BITS - value.leading_zeros()) as usize,
            _ => panic!(
                "can't
    figure out smallest width for this constant"
            ),
        }
    }

    pub fn is_unsigned(&self) -> bool {
        matches!(self, Constant::UnsignedInteger { .. })
    }

    pub fn is_signed(&self) -> bool {
        matches!(self, Constant::SignedInteger { .. })
    }
}

impl Display for Constant {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let typ = self.typ();
        match self {
            Constant::UnsignedInteger { value, .. } => write!(f, "{value:#x}{typ}"),
            Constant::SignedInteger { value, .. } => write!(f, "{value}{typ}"),
            Constant::FloatingPoint { value, .. } => write!(f, "{value}{typ}"),
            Constant::String(str) => write!(f, "{str:?}"),
            Constant::Tuple(vs) => {
                write!(f, "(").unwrap();
                vs.iter().for_each(|v| write!(f, "{v},  ").unwrap());
                write!(f, ")")
            }
            Constant::Vector(vs) => {
                write!(f, "[").unwrap();
                vs.iter().for_each(|v| write!(f, "{v},  ").unwrap());
                write!(f, "]")
            }
        }
    }
}
