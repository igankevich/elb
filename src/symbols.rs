use std::io::ErrorKind;
use std::io::Read;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::ElfRead;
use crate::ByteOrder;
use crate::Class;
use crate::ElfWrite;
use crate::Error;

/// A symbol.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Symbol {
    pub address: u64,
    pub size: u64,
    pub name_offset: u32,
    pub section_index: u16,
    pub info: u8,
    pub other: u8,
}

impl Symbol {
    /// Read from `reader`.
    pub fn read<R: ElfRead>(
        mut reader: R,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Self, Error> {
        let name_offset = reader.read_u32(byte_order)?;
        match class {
            Class::Elf32 => {
                let address = reader.read_word(class, byte_order)?;
                let size = reader.read_u32(byte_order)? as u64;
                let info = reader.read_u8()?;
                let other = reader.read_u8()?;
                let section_index = reader.read_u16(byte_order)?;
                Ok(Self {
                    name_offset,
                    address,
                    size,
                    section_index,
                    info,
                    other,
                })
            }
            Class::Elf64 => {
                let info = reader.read_u8()?;
                let other = reader.read_u8()?;
                let section_index = reader.read_u16(byte_order)?;
                let address = reader.read_word(class, byte_order)?;
                let size = reader.read_u64(byte_order)?;
                Ok(Self {
                    name_offset,
                    address,
                    size,
                    section_index,
                    info,
                    other,
                })
            }
        }
    }

    /// Write to `writer`.
    pub fn write<W: ElfWrite>(
        &self,
        mut writer: W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        writer.write_u32(byte_order, self.name_offset)?;
        match class {
            Class::Elf32 => {
                writer.write_word(class, byte_order, self.address)?;
                writer.write_u32(
                    byte_order,
                    self.size.try_into().map_err(|_| ErrorKind::InvalidData)?,
                )?;
                writer.write_u8(self.info)?;
                writer.write_u8(self.other)?;
                writer.write_u16(byte_order, self.section_index)?;
            }
            Class::Elf64 => {
                writer.write_u8(self.info)?;
                writer.write_u8(self.other)?;
                writer.write_u16(byte_order, self.section_index)?;
                writer.write_word(class, byte_order, self.address)?;
                writer.write_u64(byte_order, self.size)?;
            }
        }
        Ok(())
    }
}

/// Symbol table.
#[derive(Default)]
#[cfg_attr(test, derive(PartialEq, Eq, Debug))]
pub struct SymbolTable {
    entries: Vec<Symbol>,
}

impl SymbolTable {
    /// Create empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read table from `reader`.
    pub fn read<R: Read>(
        mut reader: R,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Self, Error> {
        let mut entries = Vec::new();
        let symbol_len = class.symbol_len();
        let mut buffer = vec![0_u8; 512 * symbol_len];
        loop {
            let n = reader.read(&mut buffer[..])?;
            if n == 0 {
                break;
            }
            let mut slice = &buffer[..n];
            for _ in 0..n / symbol_len {
                let symbol = Symbol::read(&slice[..symbol_len], class, byte_order)?;
                entries.push(symbol);
                slice = &slice[symbol_len..];
            }
        }
        Ok(Self { entries })
    }

    /// Write table to `writer`.
    pub fn write<W: ElfWrite>(
        &self,
        mut writer: W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error>
    where
        for<'a> &'a mut W: ElfWrite,
    {
        for symbol in self.entries.iter() {
            symbol.write(&mut writer, class, byte_order)?;
        }
        Ok(())
    }
}

impl Deref for SymbolTable {
    type Target = Vec<Symbol>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl DerefMut for SymbolTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    use arbitrary::Unstructured;
    use arbtest::arbtest;

    use crate::constants::*;

    #[test]
    fn symbol_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let expected = Symbol::arbitrary(u, class)?;
            let mut cursor = Cursor::new(Vec::new());
            expected
                .write(&mut cursor, class, byte_order)
                .inspect_err(|e| panic!("Failed to write {:#?}: {e}", expected))
                .unwrap();
            let bytes = cursor.into_inner();
            let actual = Symbol::read(&bytes[..], class, byte_order).unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    #[test]
    fn symbol_table_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let expected = SymbolTable::arbitrary(u, class)?;
            let mut cursor = Cursor::new(Vec::new());
            expected
                .write(&mut cursor, class, byte_order)
                .inspect_err(|e| panic!("Failed to write {:#?}: {e}", expected))
                .unwrap();
            cursor.set_position(0);
            let actual = SymbolTable::read(&mut cursor, class, byte_order)
                .inspect_err(|e| panic!("Failed to read {:#?}: {e}", expected))
                .unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    impl Symbol {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            Ok(match class {
                Class::Elf32 => Self {
                    address: u.arbitrary::<u32>()?.into(),
                    size: u.arbitrary::<u32>()?.into(),
                    name_offset: u.arbitrary()?,
                    section_index: u.arbitrary()?,
                    info: u.arbitrary()?,
                    other: u.arbitrary()?,
                },
                Class::Elf64 => Self {
                    address: u.arbitrary()?,
                    size: u.arbitrary()?,
                    name_offset: u.arbitrary()?,
                    section_index: u.arbitrary()?,
                    info: u.arbitrary()?,
                    other: u.arbitrary()?,
                },
            })
        }
    }

    impl SymbolTable {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let num_entries = u.arbitrary_len::<[u8; SYMBOL_LEN_64]>()?;
            let mut entries = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(Symbol::arbitrary(u, class)?);
            }
            Ok(Self { entries })
        }
    }
}
