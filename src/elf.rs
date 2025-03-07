use alloc::ffi::CString;
use alloc::vec::Vec;
use core::ffi::CStr;

use crate::constants::*;
use crate::BlockRead;
use crate::BlockWrite;
use crate::DynamicTable;
use crate::ElfRead;
use crate::ElfSeek;
use crate::ElfWrite;
use crate::Error;
use crate::Header;
use crate::ProgramHeader;
use crate::SectionHeader;
use crate::SectionKind;
use crate::StringTable;

/// ELF file.
#[derive(Debug)]
pub struct Elf {
    /// File header.
    pub header: Header,
    /// Program header (file segment list).
    pub segments: ProgramHeader,
    /// Section header (file section list).
    pub sections: SectionHeader,
    page_size: u64,
}

impl Elf {
    /// Read ELF from `reader` without validation.
    pub fn read_unchecked<R: ElfRead + ElfSeek>(
        reader: &mut R,
        page_size: u64,
    ) -> Result<Self, Error> {
        reader.seek(0)?;
        let header = Header::read(reader)?;
        reader.seek(header.program_header_offset)?;
        let segments = ProgramHeader::read(
            reader,
            header.class,
            header.byte_order,
            header.program_header_len(),
        )?;
        reader.seek(header.section_header_offset)?;
        let sections = SectionHeader::read(
            reader,
            header.class,
            header.byte_order,
            header.section_header_len(),
        )?;
        Ok(Self {
            header,
            segments,
            sections,
            page_size,
        })
    }

    /// Read ELF from `reader` with validation.
    ///
    /// Page size is used during the validation.
    pub fn read<R: ElfRead + ElfSeek>(reader: &mut R, page_size: u64) -> Result<Self, Error> {
        let elf = Self::read_unchecked(reader, page_size)?;
        elf.check()?;
        Ok(elf)
    }

    /// Validate and write ELF to `writer`.
    pub fn write<W: ElfWrite + ElfSeek>(self, writer: &mut W) -> Result<(), Error> {
        self.check()?;
        self.write_unchecked(writer)
    }

    /// Write ELF to `writer` without checking.
    pub fn write_unchecked<W: ElfWrite + ElfSeek>(self, writer: &mut W) -> Result<(), Error> {
        writer.seek(0)?;
        self.header.write(writer)?;
        writer.seek(self.header.program_header_offset)?;
        self.segments
            .write(writer, self.header.class, self.header.byte_order)?;
        writer.seek(self.header.section_header_offset)?;
        self.sections
            .write(writer, self.header.class, self.header.byte_order)?;
        Ok(())
    }

    /// Check consistency of the data.
    ///
    /// Validates consistency of sections, segments and their contents.
    pub fn check(&self) -> Result<(), Error> {
        self.header.check()?;
        self.segments.check(&self.header, self.page_size)?;
        self.sections.check(&self.header, &self.segments)?;
        assert_eq!(self.sections.len(), self.header.num_sections as usize);
        assert_eq!(self.segments.len(), self.header.num_segments as usize);
        Ok(())
    }

    /// Read string table containing section names.
    pub fn read_section_names<F: ElfRead + ElfSeek>(
        &self,
        file: &mut F,
    ) -> Result<Option<StringTable>, Error> {
        let Some(section) = self.sections.get(self.header.section_names_index as usize) else {
            return Ok(None);
        };
        Ok(Some(section.read_content(
            file,
            self.header.class,
            self.header.byte_order,
        )?))
    }

    /// Read dynamic table.
    pub fn read_dynamic_table<F: ElfRead + ElfSeek>(
        &self,
        file: &mut F,
    ) -> Result<Option<DynamicTable>, Error> {
        let Some(i) = self
            .sections
            .iter()
            .position(|section| section.kind == SectionKind::Dynamic)
        else {
            return Ok(None);
        };
        let section = &self.sections[i];
        file.seek(section.offset)?;
        let table = DynamicTable::read(
            file,
            self.header.class,
            self.header.byte_order,
            section.size,
        )?;
        Ok(Some(table))
    }

    /// Read dynamic string table.
    pub fn read_dynamic_string_table<F: ElfRead + ElfSeek>(
        &self,
        file: &mut F,
    ) -> Result<Option<StringTable>, Error> {
        let Some(names) = self.read_section_names(file)? else {
            return Ok(None);
        };
        let table = match self.sections.iter().position(|section| {
            Some(DYNSTR_SECTION) == names.get_string(section.name_offset as usize)
        }) {
            Some(i) => {
                self.sections[i].read_content(file, self.header.class, self.header.byte_order)?
            }
            None => return Ok(None),
        };
        Ok(Some(table))
    }

    /// Read the interpreter.
    pub fn read_interpreter<F: ElfRead + ElfSeek>(
        &self,
        names: &StringTable,
        file: &mut F,
    ) -> Result<Option<CString>, Error> {
        let Some(interp) = self.read_section(INTERP_SECTION, names, file)? else {
            return Ok(None);
        };
        Ok(Some(CString::from_vec_with_nul(interp)?))
    }

    /// Read the contents of the specified by name.
    pub fn read_section<R: ElfRead + ElfSeek>(
        &self,
        name: &CStr,
        names: &StringTable,
        file: &mut R,
    ) -> Result<Option<Vec<u8>>, Error> {
        let Some(i) = self
            .sections
            .iter()
            .position(|section| Some(name) == names.get_string(section.name_offset as usize))
        else {
            return Ok(None);
        };
        Ok(Some(self.sections[i].read_content(
            file,
            self.header.class,
            self.header.byte_order,
        )?))
    }

    /// Get page size specified on creation.
    pub fn page_size(&self) -> u64 {
        self.page_size
    }
}
