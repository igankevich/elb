use alloc::vec::Vec;
use core::ffi::CStr;

use crate::BlockIo;
use crate::ElfRead;
use crate::ElfSeek;
use crate::ElfWrite;
use crate::Error;
use crate::Header;
use crate::ProgramHeader;
use crate::SectionHeader;
use crate::StringTable;

#[derive(Debug)]
pub struct Elf {
    pub header: Header,
    pub segments: ProgramHeader,
    pub sections: SectionHeader,
    page_size: u64,
}

impl Elf {
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

    pub fn read<R: ElfRead + ElfSeek>(reader: &mut R, page_size: u64) -> Result<Self, Error> {
        let elf = Self::read_unchecked(reader, page_size)?;
        elf.validate()?;
        Ok(elf)
    }

    pub fn write<W: ElfWrite + ElfSeek>(self, writer: &mut W) -> Result<(), Error> {
        self.validate()?;
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

    pub fn validate(&self) -> Result<(), Error> {
        self.header.check()?;
        self.segments.validate(&self.header, self.page_size)?;
        self.sections.validate(&self.header, &self.segments)?;
        assert_eq!(self.sections.len(), self.header.num_sections as usize);
        assert_eq!(self.segments.len(), self.header.num_segments as usize);
        Ok(())
    }

    pub fn read_section_names<F: ElfRead + ElfSeek>(
        &self,
        file: &mut F,
    ) -> Result<StringTable, Error> {
        let section = self.sections.get(self.header.section_names_index as usize);
        if let Some(section) = section {
            Ok(section.read_content(file)?.into())
        } else {
            Ok(Default::default())
        }
    }

    pub fn read_section<R: ElfRead + ElfSeek>(
        &self,
        name: &CStr,
        file: &mut R,
    ) -> Result<Option<Vec<u8>>, Error> {
        let names = self.read_section_names(file)?;
        let i = self
            .sections
            .iter()
            .position(|section| Some(name) == names.get_string(section.name_offset as usize));
        match i {
            Some(i) => Ok(Some(self.sections[i].read_content(file)?)),
            None => Ok(None),
        }
    }

    pub fn page_size(&self) -> u64 {
        self.page_size
    }
}
