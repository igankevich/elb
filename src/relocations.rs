use alloc::vec::Vec;
use core::ops::Deref;
use core::ops::DerefMut;

use crate::BlockRead;
use crate::BlockWrite;
use crate::ByteOrder;
use crate::Class;
use crate::ElfRead;
use crate::ElfWrite;
use crate::EntityIo;
use crate::Error;

/// Relocation without an addend.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Rel {
    /// The offset from the beginning of the section.
    pub offset: u64,
    /// Symbol index.
    pub symbol: u32,
    /// Relocation type.
    pub kind: u32,
}

impl Rel {
    const fn info(&self, class: Class) -> u64 {
        match class {
            Class::Elf32 => ((self.symbol << 8) | (self.kind & 0xff)) as u64,
            Class::Elf64 => ((self.symbol as u64) << 32) | self.kind as u64,
        }
    }
}

impl EntityIo for Rel {
    fn read<R: ElfRead>(
        reader: &mut R,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Self, Error> {
        let offset;
        let info;
        match class {
            Class::Elf32 => {
                offset = reader.read_u32(byte_order)?.into();
                info = reader.read_u32(byte_order)?.into();
            }
            Class::Elf64 => {
                offset = reader.read_u64(byte_order)?;
                info = reader.read_u64(byte_order)?;
            }
        }
        let symbol = to_symbol(info, class);
        let kind = to_kind(info, class);
        Ok(Self {
            offset,
            symbol,
            kind,
        })
    }

    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        let info = self.info(class);
        match class {
            Class::Elf32 => {
                writer.write_u32_as_u64(byte_order, self.offset)?;
                writer.write_u32_as_u64(byte_order, info)?;
            }
            Class::Elf64 => {
                writer.write_u64(byte_order, self.offset)?;
                writer.write_u64(byte_order, info)?;
            }
        }
        Ok(())
    }
}

/// Relocation with an addend.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct RelA {
    /// Relocation without an addend.
    pub rel: Rel,
    /// The constant addend.
    pub addend: i64,
}

impl EntityIo for RelA {
    fn read<R: ElfRead>(
        reader: &mut R,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Self, Error> {
        let rel = Rel::read(reader, class, byte_order)?;
        let addend = match class {
            Class::Elf32 => reader.read_i32(byte_order)?.into(),
            Class::Elf64 => reader.read_i64(byte_order)?,
        };
        Ok(Self { rel, addend })
    }

    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        self.rel.write(writer, class, byte_order)?;
        match class {
            Class::Elf32 => {
                writer.write_i32_as_i64(byte_order, self.addend)?;
            }
            Class::Elf64 => {
                writer.write_i64(byte_order, self.addend)?;
            }
        }
        Ok(())
    }
}

macro_rules! define_rel_table {
    ($table: ident, $rel: ident, $rel_len: ident) => {
        #[derive(Default)]
        #[cfg_attr(test, derive(PartialEq, Eq, Debug))]
        /// Relocation table.
        pub struct $table {
            entries: Vec<$rel>,
        }

        impl $table {
            /// Create empty table.
            pub fn new() -> Self {
                Self::default()
            }
        }

        impl BlockRead for $table {
            fn read<R: ElfRead>(
                reader: &mut R,
                class: Class,
                byte_order: ByteOrder,
                len: u64,
            ) -> Result<Self, Error> {
                let mut entries = Vec::new();
                let rel_len = class.$rel_len();
                for _ in 0..len / rel_len as u64 {
                    let relocation = $rel::read(reader, class, byte_order)?;
                    entries.push(relocation);
                }
                Ok(Self { entries })
            }
        }

        impl BlockWrite for $table {
            fn write<W: ElfWrite>(
                &self,
                writer: &mut W,
                class: Class,
                byte_order: ByteOrder,
            ) -> Result<(), Error> {
                for relocation in self.entries.iter() {
                    relocation.write(writer, class, byte_order)?;
                }
                Ok(())
            }
        }

        impl Deref for $table {
            type Target = Vec<$rel>;
            fn deref(&self) -> &Self::Target {
                &self.entries
            }
        }

        impl DerefMut for $table {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.entries
            }
        }
    };
}

define_rel_table!(RelTable, Rel, rel_len);
define_rel_table!(RelaTable, RelA, rela_len);

const fn to_symbol(info: u64, class: Class) -> u32 {
    match class {
        Class::Elf32 => (info as u32) >> 8,
        Class::Elf64 => (info >> 32) as u32,
    }
}

const fn to_kind(info: u64, class: Class) -> u32 {
    match class {
        Class::Elf32 => (info & 0xff) as u32,
        Class::Elf64 => (info & 0xffff_ffff) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use arbitrary::Unstructured;

    use crate::constants::*;
    use crate::test::test_block_io;
    use crate::test::test_entity_io;
    use crate::test::ArbitraryWithClass;

    #[test]
    fn relocation_io() {
        test_entity_io::<Rel>();
    }

    #[test]
    fn relocation_a_io() {
        test_entity_io::<RelA>();
    }

    #[test]
    fn relocation_table_io() {
        test_block_io::<RelTable>();
    }

    #[test]
    fn relocation_a_table_io() {
        test_block_io::<RelaTable>();
    }

    impl ArbitraryWithClass<'_> for Rel {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            Ok(match class {
                Class::Elf32 => Self {
                    offset: u.arbitrary::<u32>()?.into(),
                    // 24 bits
                    symbol: u.int_in_range(0..=0xff_ffff)?,
                    kind: u.arbitrary::<u8>()?.into(),
                },
                Class::Elf64 => Self {
                    offset: u.arbitrary()?,
                    symbol: u.arbitrary::<u32>()?,
                    kind: u.arbitrary::<u32>()?,
                },
            })
        }
    }

    impl ArbitraryWithClass<'_> for RelA {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            Ok(match class {
                Class::Elf32 => Self {
                    rel: Rel::arbitrary(u, class)?,
                    addend: u.arbitrary::<i32>()?.into(),
                },
                Class::Elf64 => Self {
                    rel: Rel::arbitrary(u, class)?,
                    addend: u.arbitrary()?,
                },
            })
        }
    }

    impl ArbitraryWithClass<'_> for RelTable {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let num_entries = u.arbitrary_len::<[u8; REL_LEN_64]>()?;
            let mut entries = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(Rel::arbitrary(u, class)?);
            }
            Ok(Self { entries })
        }
    }

    impl ArbitraryWithClass<'_> for RelaTable {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let num_entries = u.arbitrary_len::<[u8; RELA_LEN_64]>()?;
            let mut entries = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(RelA::arbitrary(u, class)?);
            }
            Ok(Self { entries })
        }
    }
}
