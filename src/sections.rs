use alloc::vec::Vec;
use core::ops::Deref;
use core::ops::DerefMut;
use core::ops::Range;

use crate::check_u32;
use crate::constants::*;
use crate::zero;
use crate::BlockRead;
use crate::BlockWrite;
use crate::ByteOrder;
use crate::Class;
use crate::ElfRead;
use crate::ElfSeek;
use crate::ElfWrite;
use crate::EntityIo;
use crate::Error;
use crate::FileKind;
use crate::Header;
use crate::ProgramHeader;
use crate::SectionFlags;
use crate::SectionKind;
use crate::SegmentKind;

/// Sections.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct SectionHeader {
    entries: Vec<Section>,
}

impl BlockRead for SectionHeader {
    fn read<R: ElfRead>(
        reader: &mut R,
        class: Class,
        byte_order: ByteOrder,
        len: u64,
    ) -> Result<Self, Error> {
        let num_sections = len / class.section_len() as u64;
        let mut entries = Vec::with_capacity(num_sections as usize);
        for _ in 0..num_sections {
            let entry = Section::read(reader, class, byte_order)?;
            entries.push(entry);
        }
        let ret = Self { entries };
        Ok(ret)
    }
}

impl BlockWrite for SectionHeader {
    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        for entry in self.entries.iter() {
            entry.write(writer, class, byte_order)?;
        }
        Ok(())
    }
}

impl SectionHeader {
    /// Check sections.
    pub fn check(&self, header: &Header, program_header: &ProgramHeader) -> Result<(), Error> {
        if let Some(section) = self.entries.first() {
            if section.kind != SectionKind::Null {
                return Err(Error::InvalidFirstSectionKind(section.kind));
            }
        }
        if (SECTION_RESERVED_MIN..=SECTION_RESERVED_MAX).contains(&self.entries.len()) {
            return Err(Error::TooManySections(self.entries.len()));
        }
        self.check_count()?;
        for section in self.entries.iter() {
            section.check(header, program_header)?;
        }
        Ok(())
    }

    pub(crate) fn free<W: ElfWrite + ElfSeek>(
        &mut self,
        writer: &mut W,
        i: usize,
    ) -> Result<Section, Error> {
        let section = std::mem::take(&mut self.entries[i]);
        log::trace!(
            "Freeing file block {:#x}..{:#x}",
            section.offset,
            section.offset + section.size
        );
        log::trace!(
            "Freeing memory block {:#x}..{:#x}",
            section.virtual_address,
            section.virtual_address + section.size
        );
        section.clear_content(writer)?;
        Ok(section)
    }

    pub(crate) fn add(&mut self, section: Section) -> usize {
        // Always append NULL sections.
        if section.kind == SectionKind::Null {
            let i = self.entries.len();
            self.entries.push(section);
            return i;
        }
        // The first section should always be NULL.
        // It is used for e.g. storing the no. of segments if it overflows u16.
        if self.entries.is_empty() {
            self.entries.push(Section::null());
        }
        let skip = 1;
        let spare_index = self
            .entries
            .iter()
            // The first NULL section can't be reused.
            .skip(skip)
            .position(|section| section.kind == SectionKind::Null);
        match spare_index {
            Some(i) => {
                let i = i + skip;
                log::trace!("Found NULL section at {i}");
                // Replace null section with the new one.
                self.entries[i] = section;
                i
            }
            None => {
                // No null sections found. Append the new one.
                let i = self.entries.len();
                self.entries.push(section);
                i
            }
        }
    }

    pub(crate) fn finish(&mut self) {
        if self.entries.is_empty() {
            self.entries.push(Section::null());
        }
    }

    fn check_count(&self) -> Result<(), Error> {
        use SectionKind::*;
        for kind in [Hash, Dynamic] {
            if self
                .entries
                .iter()
                .filter(|section| section.kind == kind)
                .count()
                > 1
            {
                return Err(Error::MultipleSections(kind));
            }
        }
        Ok(())
    }
}

impl Deref for SectionHeader {
    type Target = Vec<Section>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl DerefMut for SectionHeader {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}

/// Section.
///
/// Dynamic loader maps sections into virtual address space of a program as part of segments.
/// Usually sections are part of [segments](crate::Segment), however, some section types exist on
/// their own.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Section {
    /// Offset of the section name in the section that stores section names.
    ///
    /// You can find the index of this section via
    /// [`Header::section_names_index`](crate::Header::section_names_index).
    pub name_offset: u32,
    /// Section type.
    pub kind: SectionKind,
    /// Section flags.
    pub flags: SectionFlags,
    /// Virtual address (in-memory offset).
    pub virtual_address: u64,
    /// In-file offset.
    pub offset: u64,
    /// Section size.
    pub size: u64,
    /// Optional index of the related section.
    pub link: u32,
    /// Extra information.
    ///
    /// Depends on the section type.
    pub info: u32,
    /// Alignment.
    ///
    /// Only virtual address has alignment constraints.
    pub align: u64,
    /// The size of the entry in the references table.
    ///
    /// Only relevant for sections that reference tables.
    pub entry_len: u64,
}

impl Section {
    /// Create `NULL` section.
    pub fn null() -> Self {
        Self {
            name_offset: 0,
            kind: SectionKind::Null,
            flags: SectionFlags::from_bits_retain(0),
            virtual_address: 0,
            offset: 0,
            size: 0,
            link: 0,
            info: 0,
            align: 0,
            entry_len: 0,
        }
    }
}

impl EntityIo for Section {
    fn read<R: ElfRead>(
        reader: &mut R,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Self, Error> {
        let name_offset = reader.read_u32(byte_order)?;
        let kind: SectionKind = reader.read_u32(byte_order)?.into();
        let flags = reader.read_word(class, byte_order)?;
        let virtual_address = reader.read_word(class, byte_order)?;
        let offset = reader.read_word(class, byte_order)?;
        let size = reader.read_word(class, byte_order)?;
        let link = reader.read_u32(byte_order)?;
        let info = reader.read_u32(byte_order)?;
        let align = reader.read_word(class, byte_order)?;
        let entry_len = reader.read_word(class, byte_order)?;
        Ok(Self {
            name_offset,
            kind,
            flags: SectionFlags::from_bits_retain(flags),
            virtual_address,
            offset,
            size,
            link,
            info,
            align,
            entry_len,
        })
    }

    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        writer.write_u32(byte_order, self.name_offset)?;
        writer.write_u32(byte_order, self.kind.as_u32())?;
        writer.write_word(class, byte_order, self.flags.bits())?;
        writer.write_word(class, byte_order, self.virtual_address)?;
        writer.write_word(class, byte_order, self.offset)?;
        writer.write_word(class, byte_order, self.size)?;
        writer.write_u32(byte_order, self.link)?;
        writer.write_u32(byte_order, self.info)?;
        writer.write_word(class, byte_order, self.align)?;
        writer.write_word(class, byte_order, self.entry_len)?;
        Ok(())
    }
}

impl Section {
    /// Read section contents as bytes.
    pub fn read_content<R: ElfRead + ElfSeek, T: BlockRead>(
        &self,
        reader: &mut R,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<T, Error> {
        reader.seek(self.offset)?;
        T::read(reader, class, byte_order, self.size)
    }

    /// Write section contents.
    pub fn write_content<W: ElfWrite + ElfSeek, T: BlockWrite + ?Sized>(
        &self,
        writer: &mut W,
        class: Class,
        byte_order: ByteOrder,
        content: &T,
    ) -> Result<(), Error> {
        writer.seek(self.offset)?;
        content.write(writer, class, byte_order)?;
        Ok(())
    }

    /// Zero out the entry's content.
    pub fn clear_content<W: ElfWrite + ElfSeek>(&self, writer: &mut W) -> Result<(), Error> {
        zero(writer, self.offset, self.size)?;
        Ok(())
    }

    /// Virtual address range.
    pub const fn virtual_address_range(&self) -> Range<u64> {
        let start = self.virtual_address;
        let end = start + self.size;
        start..end
    }

    /// In-file location of the segment.
    pub const fn file_offset_range(&self) -> Range<u64> {
        if matches!(self.kind, SectionKind::NoBits) {
            return self.offset..self.offset;
        }
        let start = self.offset;
        let end = start + self.size;
        start..end
    }

    /// Check consistency.
    pub fn check(&self, header: &Header, program_header: &ProgramHeader) -> Result<(), Error> {
        if self.kind == SectionKind::Null {
            return Ok(());
        }
        self.check_overflow(header.class)?;
        self.check_align()?;
        if header.kind != FileKind::Relocatable {
            self.check_coverage(program_header)?;
        }
        Ok(())
    }

    fn check_overflow(&self, class: Class) -> Result<(), Error> {
        match class {
            Class::Elf32 => {
                check_u32(self.flags.bits(), "Section flags")?;
                check_u32(self.virtual_address, "Section virtual address")?;
                check_u32(self.offset, "Section offset")?;
                check_u32(self.size, "Section size")?;
                check_u32(self.align, "Section align")?;
                check_u32(self.entry_len, "Section entry size")?;
            }
            Class::Elf64 => {
                if self.offset.checked_add(self.size).is_none() {
                    return Err(Error::TooBig("Section in-file size"));
                }
                if self.virtual_address.checked_add(self.size).is_none() {
                    return Err(Error::TooBig("Section in-memory size"));
                }
            }
        }
        Ok(())
    }

    #[allow(unknown_lints)]
    #[allow(clippy::manual_is_multiple_of)]
    fn check_align(&self) -> Result<(), Error> {
        match self.kind {
            SectionKind::NoBits => {
                // BSS section is not stored in the file and has arbitrary offset.
            }
            _ if self.flags.contains(SectionFlags::ALLOC) => {
                let align = self.align;
                if align > 1 && self.virtual_address % align != 0 {
                    let section_start = self.virtual_address;
                    let section_end = section_start + self.size;
                    return Err(Error::MisalignedSection(section_start, section_end, align));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn check_coverage(&self, program_header: &ProgramHeader) -> Result<(), Error> {
        // TODO this is quadratic
        let section_start = self.virtual_address;
        let section_end = section_start + self.size;
        if section_start != section_end
            && self.flags.contains(SectionFlags::ALLOC)
            && self.kind != SectionKind::NoBits
            && !program_header.iter().any(|segment| {
                if segment.kind != SegmentKind::Loadable {
                    return false;
                }
                let segment_start = segment.virtual_address;
                let segment_end = segment_start + segment.memory_size;
                segment_start <= section_start
                    && section_start < segment_end
                    && section_end <= segment_end
            })
        {
            return Err(Error::SectionNotCovered(section_start, section_end));
        }
        Ok(())
    }
}

impl Default for Section {
    fn default() -> Self {
        Self::null()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use arbitrary::Unstructured;

    use crate::test::test_block_io;
    use crate::test::test_entity_io;
    use crate::test::ArbitraryWithClass;

    #[test]
    fn section_io() {
        test_entity_io::<Section>();
    }

    #[test]
    fn section_header_io() {
        test_block_io::<SectionHeader>();
    }

    impl ArbitraryWithClass<'_> for SectionHeader {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let num_entries = u.arbitrary_len::<[u8; SECTION_LEN_64]>()?;
            let mut entries = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(Section::arbitrary(u, class)?);
            }
            Ok(Self { entries })
        }
    }

    impl ArbitraryWithClass<'_> for Section {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            Ok(Self {
                name_offset: u.arbitrary()?,
                kind: u.arbitrary()?,
                flags: SectionFlags::from_bits_retain(class.arbitrary_word(u)?),
                virtual_address: class.arbitrary_word(u)?,
                offset: class.arbitrary_word(u)?,
                size: class.arbitrary_word(u)?,
                link: u.arbitrary()?,
                info: u.arbitrary()?,
                align: class.arbitrary_align(u)?,
                entry_len: class.section_len().into(),
            })
        }
    }
}
