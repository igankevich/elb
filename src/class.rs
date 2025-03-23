use crate::constants::*;
use crate::Error;

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

    pub const fn segment_len(self) -> u16 {
        match self {
            Self::Elf32 => SEGMENT_LEN_32 as u16,
            Self::Elf64 => SEGMENT_LEN_64 as u16,
        }
    }

    pub const fn section_len(self) -> u16 {
        match self {
            Self::Elf32 => SECTION_LEN_32 as u16,
            Self::Elf64 => SECTION_LEN_64 as u16,
        }
    }

    pub const fn symbol_len(self) -> usize {
        match self {
            Self::Elf32 => SYMBOL_LEN_32,
            Self::Elf64 => SYMBOL_LEN_64,
        }
    }

    pub const fn rel_len(self) -> usize {
        match self {
            Self::Elf32 => REL_LEN_32,
            Self::Elf64 => REL_LEN_64,
        }
    }

    pub const fn rela_len(self) -> usize {
        match self {
            Self::Elf32 => RELA_LEN_32,
            Self::Elf64 => RELA_LEN_64,
        }
    }

    pub const fn word_max(self) -> u64 {
        match self {
            Self::Elf32 => u32::MAX as u64,
            Self::Elf64 => u64::MAX,
        }
    }
}

impl TryFrom<u8> for Class {
    type Error = Error;
    fn try_from(other: u8) -> Result<Self, Self::Error> {
        match other {
            1 => Ok(Self::Elf32),
            2 => Ok(Self::Elf64),
            n => Err(Error::InvalidClass(n)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use arbitrary::Unstructured;

    impl Class {
        pub fn arbitrary_word(self, u: &mut Unstructured<'_>) -> arbitrary::Result<u64> {
            match self {
                Self::Elf32 => Ok(u.arbitrary::<u32>()?.into()),
                Self::Elf64 => Ok(u.arbitrary()?),
            }
        }

        pub fn arbitrary_align(self, u: &mut Unstructured<'_>) -> arbitrary::Result<u64> {
            let n = match self {
                Self::Elf32 => 31,
                Self::Elf64 => 63,
            };
            let align = 1_u64 << u.int_in_range(0..=n)?;
            Ok(align)
        }
    }
}
