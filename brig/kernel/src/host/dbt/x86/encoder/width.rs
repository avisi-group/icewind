use core::{
    cmp::Ordering,
    fmt::{self, Display, Formatter},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    _8,
    _16,
    _32,
    _64,
    _128,
}

impl PartialOrd for Width {
    fn partial_cmp(&self, other: &Width) -> Option<Ordering> {
        Some(self.cmp(&other))
    }
}

impl Ord for Width {
    fn cmp(&self, other: &Width) -> Ordering {
        self.as_u16().cmp(&other.as_u16())
    }
}

impl Width {
    pub fn from_uncanonicalized<U: Into<u64>>(bits: U) -> Result<Self, WidthError> {
        match bits.into() {
            1..=8 => Ok(Self::_8),
            9..=16 => Ok(Self::_16),
            17..=32 => Ok(Self::_32),
            33..=64 => Ok(Self::_64),
            65..=128 => Ok(Self::_128),
            0 => Err(WidthError::Zero),
            // n => Err(WidthError::Oversize(n)),
            _ => Ok(Self::_128), // todo: fix PhysicalCount and other oversized registers
        }
    }

    pub fn as_u16(&self) -> u16 {
        match self {
            Width::_8 => 8,
            Width::_16 => 16,
            Width::_32 => 32,
            Width::_64 => 64,
            Width::_128 => 128,
        }
    }
}

impl Display for Width {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u16())
    }
}

/// Width canonicalization error
#[derive(Debug, displaydoc::Display)]
pub enum WidthError {
    /// Cannot encode 0 sized width
    Zero,
    /// Width {0} too large
    Oversize(u16),
}
