use std::io::Error;
use std::io::ErrorKind;
use std::io::Write;

use crate::constants::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[repr(u8)]
pub enum Class {
    Elf32 = 1,
    Elf64 = 2,
}

impl Class {
    pub const fn word_len(self) -> usize {
        match self {
            Self::Elf32 => 4,
            Self::Elf64 => 8,
        }
    }

    pub const fn header_len(self) -> u16 {
        match self {
            Self::Elf32 => HEADER_LEN_32 as u16,
            Self::Elf64 => HEADER_LEN_64 as u16,
        }
    }

    pub const fn program_entry_len(self) -> u16 {
        match self {
            Self::Elf32 => PROGRAM_ENTRY_LEN_32 as u16,
            Self::Elf64 => PROGRAM_ENTRY_LEN_64 as u16,
        }
    }

    pub const fn section_entry_len(self) -> u16 {
        match self {
            Self::Elf32 => SECTION_ENTRY_LEN_32 as u16,
            Self::Elf64 => SECTION_ENTRY_LEN_64 as u16,
        }
    }
}

impl TryFrom<u8> for Class {
    type Error = Error;
    fn try_from(other: u8) -> Result<Self, Self::Error> {
        match other {
            1 => Ok(Self::Elf32),
            2 => Ok(Self::Elf64),
            _ => Err(ErrorKind::InvalidData.into()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[repr(u8)]
pub enum ByteOrder {
    LittleEndian = 1,
    BigEndian = 2,
}

impl ByteOrder {
    pub fn get_u16(self, data: &[u8]) -> u16 {
        match self {
            Self::LittleEndian => u16::from_le_bytes([data[0], data[1]]),
            Self::BigEndian => u16::from_be_bytes([data[0], data[1]]),
        }
    }

    pub fn write_u16<W: Write>(self, mut writer: W, value: u16) -> Result<(), Error> {
        let bytes = match self {
            Self::LittleEndian => value.to_le_bytes(),
            Self::BigEndian => value.to_be_bytes(),
        };
        writer.write_all(&bytes)
    }

    pub fn get_u32(self, data: &[u8]) -> u32 {
        match self {
            Self::LittleEndian => u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            Self::BigEndian => u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
        }
    }

    pub fn write_u32<W: Write>(self, mut writer: W, value: u32) -> Result<(), Error> {
        let bytes = match self {
            Self::LittleEndian => value.to_le_bytes(),
            Self::BigEndian => value.to_be_bytes(),
        };
        writer.write_all(&bytes)
    }
}

impl TryFrom<u8> for ByteOrder {
    type Error = Error;
    fn try_from(other: u8) -> Result<Self, Self::Error> {
        match other {
            1 => Ok(Self::LittleEndian),
            2 => Ok(Self::BigEndian),
            _ => Err(ErrorKind::InvalidData.into()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Word {
    U32(u32),
    U64(u64),
}

impl Word {
    pub fn new(class: Class, byte_order: ByteOrder, data: &[u8]) -> Word {
        match class {
            Class::Elf32 => Word::U32(match byte_order {
                ByteOrder::LittleEndian => u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
                ByteOrder::BigEndian => u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            }),
            Class::Elf64 => Word::U64(match byte_order {
                ByteOrder::LittleEndian => u64::from_le_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]),
                ByteOrder::BigEndian => u64::from_be_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]),
            }),
        }
    }

    pub fn write<W: Write>(&self, mut writer: W, byte_order: ByteOrder) -> Result<(), Error> {
        match self {
            Self::U32(x) => {
                let bytes = match byte_order {
                    ByteOrder::LittleEndian => x.to_le_bytes(),
                    ByteOrder::BigEndian => x.to_be_bytes(),
                };
                writer.write_all(&bytes)?;
            }
            Self::U64(x) => {
                let bytes = match byte_order {
                    ByteOrder::LittleEndian => x.to_le_bytes(),
                    ByteOrder::BigEndian => x.to_be_bytes(),
                };
                writer.write_all(&bytes)?;
            }
        }
        Ok(())
    }

    pub const fn from_u32(class: Class, value: u32) -> Self {
        match class {
            Class::Elf32 => Word::U32(value),
            Class::Elf64 => Word::U64(value as u64),
        }
    }

    pub fn from_u64(class: Class, value: u64) -> Option<Self> {
        match class {
            Class::Elf32 => Some(Word::U32(value.try_into().ok()?)),
            Class::Elf64 => Some(Word::U64(value)),
        }
    }

    pub const fn class(self) -> Class {
        match self {
            Self::U32(..) => Class::Elf32,
            Self::U64(..) => Class::Elf64,
        }
    }

    pub const fn max(self) -> u64 {
        match self {
            Self::U32(..) => u32::MAX as u64,
            Self::U64(..) => u64::MAX,
        }
    }

    pub const fn size(self) -> usize {
        match self {
            Self::U32(..) => 4,
            Self::U64(..) => 8,
        }
    }

    pub const fn as_u64(self) -> u64 {
        match self {
            Self::U32(x) => x as u64,
            Self::U64(x) => x,
        }
    }

    // TODO
    pub const fn as_usize(self) -> usize {
        match self {
            Self::U32(x) => x as usize,
            Self::U64(x) => x as usize,
        }
    }

    pub fn set_usize(&mut self, value: usize) -> Result<(), Error> {
        match self {
            Self::U32(x) => *x = value.try_into().map_err(|_| ErrorKind::InvalidData)?,
            Self::U64(x) => *x = value.try_into().map_err(|_| ErrorKind::InvalidData)?,
        }
        Ok(())
    }

    pub fn set_u64(&mut self, value: u64) -> Result<(), Error> {
        match self {
            Self::U32(x) => *x = value.try_into().map_err(|_| ErrorKind::InvalidData)?,
            Self::U64(x) => *x = value,
        }
        Ok(())
    }
}

impl TryFrom<Word> for u32 {
    type Error = Error;
    fn try_from(word: Word) -> Result<Self, Self::Error> {
        match word {
            Word::U32(x) => Ok(x),
            Word::U64(x) => Ok(x.try_into().map_err(|_| ErrorKind::InvalidData)?),
        }
    }
}

#[cfg(test)]
mod tests {
    use arbitrary::Unstructured;

    use super::*;

    impl Word {
        pub fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let ret = match class {
                Class::Elf32 => Word::U32(u.arbitrary()?),
                Class::Elf64 => Word::U64(u.arbitrary()?),
            };
            Ok(ret)
        }
    }
}
