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
use crate::SymbolBinding;
use crate::SymbolKind;
use crate::SymbolVisibility;

/// A symbol.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Symbol {
    /// Address.
    pub address: u64,
    /// Associated size of zero if the size is unknown.
    pub size: u64,
    /// The offset of the name in the symbol string table.
    pub name_offset: u32,
    /// Relevant section index.
    pub section_index: u16,
    /// Binding.
    pub binding: SymbolBinding,
    /// Type.
    pub kind: SymbolKind,
    /// Visibility.
    pub visibility: SymbolVisibility,
}

impl Symbol {
    const fn info(&self) -> u8 {
        self.binding.to_info_bits() | self.kind.to_info_bits()
    }
}

impl EntityIo for Symbol {
    fn read<R: ElfRead>(
        reader: &mut R,
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
                    binding: SymbolBinding::from_info(info),
                    kind: SymbolKind::from_info(info),
                    visibility: SymbolVisibility::from_other(other),
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
                    binding: SymbolBinding::from_info(info),
                    kind: SymbolKind::from_info(info),
                    visibility: SymbolVisibility::from_other(other),
                })
            }
        }
    }

    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        writer.write_u32(byte_order, self.name_offset)?;
        match class {
            Class::Elf32 => {
                writer.write_word(class, byte_order, self.address)?;
                writer.write_u32_as_u64(byte_order, self.size)?;
                writer.write_u8(self.info())?;
                writer.write_u8(self.visibility as u8)?;
                writer.write_u16(byte_order, self.section_index)?;
            }
            Class::Elf64 => {
                writer.write_u8(self.info())?;
                writer.write_u8(self.visibility as u8)?;
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
}

impl BlockRead for SymbolTable {
    fn read<R: ElfRead>(
        reader: &mut R,
        class: Class,
        byte_order: ByteOrder,
        len: u64,
    ) -> Result<Self, Error> {
        let mut entries = Vec::new();
        let symbol_len = class.symbol_len();
        for _ in 0..len / symbol_len as u64 {
            let symbol = Symbol::read(reader, class, byte_order)?;
            entries.push(symbol);
        }
        Ok(Self { entries })
    }
}

impl BlockWrite for SymbolTable {
    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        for symbol in self.entries.iter() {
            symbol.write(writer, class, byte_order)?;
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

    use arbitrary::Unstructured;

    use crate::constants::*;
    use crate::test::test_block_io;
    use crate::test::test_entity_io;
    use crate::test::ArbitraryWithClass;

    #[test]
    fn symbol_io() {
        test_entity_io::<Symbol>();
    }

    #[test]
    fn symbol_table_io() {
        test_block_io::<SymbolTable>();
    }

    impl ArbitraryWithClass<'_> for Symbol {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let info = u.arbitrary()?;
            Ok(match class {
                Class::Elf32 => Self {
                    address: u.arbitrary::<u32>()?.into(),
                    size: u.arbitrary::<u32>()?.into(),
                    name_offset: u.arbitrary()?,
                    section_index: u.arbitrary()?,
                    binding: SymbolBinding::from_info(info),
                    kind: SymbolKind::from_info(info),
                    visibility: u.arbitrary()?,
                },
                Class::Elf64 => Self {
                    address: u.arbitrary()?,
                    size: u.arbitrary()?,
                    name_offset: u.arbitrary()?,
                    section_index: u.arbitrary()?,
                    binding: SymbolBinding::from_info(info),
                    kind: SymbolKind::from_info(info),
                    visibility: u.arbitrary()?,
                },
            })
        }
    }

    impl ArbitraryWithClass<'_> for SymbolTable {
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
