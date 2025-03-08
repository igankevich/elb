use crate::Error;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[repr(u8)]
pub enum ByteOrder {
    LittleEndian = 1,
    BigEndian = 2,
}

impl TryFrom<u8> for ByteOrder {
    type Error = Error;
    fn try_from(other: u8) -> Result<Self, Self::Error> {
        match other {
            1 => Ok(Self::LittleEndian),
            2 => Ok(Self::BigEndian),
            n => Err(Error::InvalidByteOrder(n)),
        }
    }
}
