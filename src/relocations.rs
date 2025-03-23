use std::ops::Deref;
use std::ops::DerefMut;

use crate::ElfRead;
use crate::ByteOrder;
use crate::Class;
use crate::ElfWrite;
use crate::Error;

/// Relocation without an addend.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Relocation {
    pub offset: u64,
    pub info: u64,
}

impl Relocation {
    pub fn read<R: ElfRead>(
        mut reader: R,
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
        Ok(Self { offset, info })
    }

    pub fn write<W: ElfWrite>(
        &self,
        mut writer: W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        match class {
            Class::Elf32 => {
                writer.write_u32_as_u64(byte_order, self.offset)?;
                writer.write_u32_as_u64(byte_order, self.info)?;
            }
            Class::Elf64 => {
                writer.write_u64(byte_order, self.offset)?;
                writer.write_u64(byte_order, self.info)?;
            }
        }
        Ok(())
    }
}

/// Relocation with an addend.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct RelocationA {
    pub offset: u64,
    pub info: u64,
    pub addend: i64,
}

impl RelocationA {
    pub fn read<R: ElfRead>(
        mut reader: R,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Self, Error> {
        let offset;
        let info;
        let addend;
        match class {
            Class::Elf32 => {
                offset = reader.read_u32(byte_order)?.into();
                info = reader.read_u32(byte_order)?.into();
                addend = reader.read_i32(byte_order)?.into();
            }
            Class::Elf64 => {
                offset = reader.read_u64(byte_order)?;
                info = reader.read_u64(byte_order)?;
                addend = reader.read_i64(byte_order)?;
            }
        }
        Ok(Self {
            offset,
            info,
            addend,
        })
    }

    pub fn write<W: ElfWrite>(
        &self,
        mut writer: W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        match class {
            Class::Elf32 => {
                writer.write_u32_as_u64(byte_order, self.offset)?;
                writer.write_u32_as_u64(byte_order, self.info)?;
                writer.write_i32_as_i64(byte_order, self.addend)?;
            }
            Class::Elf64 => {
                writer.write_u64(byte_order, self.offset)?;
                writer.write_u64(byte_order, self.info)?;
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
        pub struct $table {
            entries: Vec<$rel>,
        }

        impl $table {
            pub fn new() -> Self {
                Self::default()
            }

            pub fn read<R: ElfRead>(
                mut reader: R,
                class: Class,
                byte_order: ByteOrder,
                len: u64,
            ) -> Result<Self, Error>
            where
                for<'a> &'a mut R: ElfRead,
            {
                let mut entries = Vec::new();
                let rel_len = class.$rel_len();
                for _ in 0..len / rel_len as u64 {
                    let relocation = $rel::read(&mut reader, class, byte_order)?;
                    entries.push(relocation);
                }
                Ok(Self { entries })
            }

            pub fn write<W: ElfWrite>(
                &self,
                mut writer: W,
                class: Class,
                byte_order: ByteOrder,
            ) -> Result<(), Error>
            where
                for<'a> &'a mut W: ElfWrite,
            {
                for relocation in self.entries.iter() {
                    relocation.write(&mut writer, class, byte_order)?;
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

define_rel_table!(RelTable, Relocation, rel_len);
define_rel_table!(RelaTable, RelocationA, rela_len);

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    use arbitrary::Unstructured;
    use arbtest::arbtest;

    use crate::constants::*;

    #[test]
    fn relocation_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let expected = Relocation::arbitrary(u, class)?;
            let mut cursor = Cursor::new(Vec::new());
            expected
                .write(&mut cursor, class, byte_order)
                .inspect_err(|e| panic!("Failed to write {:#?}: {e}", expected))
                .unwrap();
            let bytes = cursor.into_inner();
            let actual = Relocation::read(&bytes[..], class, byte_order).unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    #[test]
    fn relocation_a_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let expected = RelocationA::arbitrary(u, class)?;
            let mut cursor = Cursor::new(Vec::new());
            expected
                .write(&mut cursor, class, byte_order)
                .inspect_err(|e| panic!("Failed to write {:#?}: {e}", expected))
                .unwrap();
            let bytes = cursor.into_inner();
            let actual = RelocationA::read(&bytes[..], class, byte_order).unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    #[test]
    fn relocation_table_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let expected = RelTable::arbitrary(u, class)?;
            let mut cursor = Cursor::new(Vec::new());
            expected
                .write(&mut cursor, class, byte_order)
                .inspect_err(|e| panic!("Failed to write {:#?}: {e}", expected))
                .unwrap();
            let len = cursor.position();
            cursor.set_position(0);
            let actual = RelTable::read(&mut cursor, class, byte_order, len)
                .inspect_err(|e| panic!("Failed to read {:#?}: {e}", expected))
                .unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    #[test]
    fn relocation_a_table_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let expected = RelaTable::arbitrary(u, class)?;
            let mut cursor = Cursor::new(Vec::new());
            expected
                .write(&mut cursor, class, byte_order)
                .inspect_err(|e| panic!("Failed to write {:#?}: {e}", expected))
                .unwrap();
            let len = cursor.position();
            cursor.set_position(0);
            let actual = RelaTable::read(&mut cursor, class, byte_order, len)
                .inspect_err(|e| panic!("Failed to read {:#?}: {e}", expected))
                .unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    impl Relocation {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            Ok(match class {
                Class::Elf32 => Self {
                    offset: u.arbitrary::<u32>()?.into(),
                    info: u.arbitrary::<u32>()?.into(),
                },
                Class::Elf64 => Self {
                    offset: u.arbitrary()?,
                    info: u.arbitrary()?,
                },
            })
        }
    }

    impl RelocationA {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            Ok(match class {
                Class::Elf32 => Self {
                    offset: u.arbitrary::<u32>()?.into(),
                    info: u.arbitrary::<u32>()?.into(),
                    addend: u.arbitrary::<i32>()?.into(),
                },
                Class::Elf64 => Self {
                    offset: u.arbitrary()?,
                    info: u.arbitrary()?,
                    addend: u.arbitrary()?,
                },
            })
        }
    }

    impl RelTable {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let num_entries = u.arbitrary_len::<[u8; REL_LEN_64]>()?;
            let mut entries = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(Relocation::arbitrary(u, class)?);
            }
            Ok(Self { entries })
        }
    }

    impl RelaTable {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let num_entries = u.arbitrary_len::<[u8; RELA_LEN_64]>()?;
            let mut entries = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(RelocationA::arbitrary(u, class)?);
            }
            Ok(Self { entries })
        }
    }
}
