use std::io::BufWriter;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::constants::*;
use crate::io::*;
use crate::ByteOrder;
use crate::Class;
use crate::Error;

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
    pub fn from_bytes(buf: &[u8], class: Class, byte_order: ByteOrder) -> Self {
        assert_eq!(class.symbol_len(), buf.len());
        let word_len = class.word_len();
        let mut slice = buf;
        let name_offset = get_u32(slice, byte_order);
        slice = &slice[4..];
        match class {
            Class::Elf32 => {
                let address = get_word(class, byte_order, slice);
                slice = &slice[word_len..];
                let size = get_u32(slice, byte_order) as u64;
                slice = &slice[4..];
                let info = slice[0];
                let other = slice[1];
                slice = &slice[2..];
                let section_index = get_u16(slice, byte_order);
                slice = &slice[2..];
                assert_eq!(0, slice.len());
                Self {
                    name_offset,
                    address,
                    size,
                    section_index,
                    info,
                    other,
                }
            }
            Class::Elf64 => {
                let info = slice[0];
                let other = slice[1];
                slice = &slice[2..];
                let section_index = get_u16(slice, byte_order);
                slice = &slice[2..];
                let address = get_word(class, byte_order, slice);
                slice = &slice[word_len..];
                let size = get_u64(slice, byte_order);
                slice = &slice[8..];
                assert_eq!(0, slice.len());
                Self {
                    name_offset,
                    address,
                    size,
                    section_index,
                    info,
                    other,
                }
            }
        }
    }

    pub fn write<W: Write>(
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

#[derive(Default)]
#[cfg_attr(test, derive(PartialEq, Eq, Debug))]
pub struct SymbolTable {
    entries: Vec<Symbol>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self::default()
    }

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
                let symbol = Symbol::from_bytes(&slice[..symbol_len], class, byte_order);
                entries.push(symbol);
                slice = &slice[symbol_len..];
            }
        }
        Ok(Self { entries })
    }

    pub fn write<W: Write>(
        &self,
        writer: W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        let mut writer = BufWriter::new(writer);
        let symbol_len = class.symbol_len();
        let mut buf = [0_u8; MAX_SYMBOL_LEN];
        for symbol in self.entries.iter() {
            symbol.write(&mut buf[..symbol_len], class, byte_order)?;
            writer.write_all(&buf[..symbol_len])?;
        }
        writer.flush()?;
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

// TODO Don't read the table, iterate over entries using bufreader?

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    use arbitrary::Unstructured;
    use arbtest::arbtest;

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
            let actual = Symbol::from_bytes(&bytes, class, byte_order);
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
            let num_entries = u.arbitrary_len::<[u8; MAX_SYMBOL_LEN]>()?;
            let mut entries = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(Symbol::arbitrary(u, class)?);
            }
            Ok(Self { entries })
        }
    }
}
