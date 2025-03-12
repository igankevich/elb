use std::ffi::CStr;
use std::io::Read;
use std::io::Seek;
use std::io::Write;

use crate::constants::*;
use crate::Allocations;
use crate::DynamicEntryKind;
use crate::DynamicTable;
use crate::Error;
use crate::Header;
use crate::ProgramHeader;
use crate::Section;
use crate::SectionFlags;
use crate::SectionHeader;
use crate::SectionKind;
use crate::Segment;
use crate::SegmentFlags;
use crate::SegmentKind;
use crate::StringTable;

#[derive(Debug)]
pub struct Elf {
    pub header: Header,
    pub segments: ProgramHeader,
    pub sections: SectionHeader,
}

impl Elf {
    pub fn read_unchecked<R: Read + Seek>(mut reader: R) -> Result<Self, Error> {
        let header = Header::read(&mut reader)?;
        let segments = ProgramHeader::read(&mut reader, &header)?;
        let sections = SectionHeader::read(&mut reader, &header)?;
        Ok(Self {
            header,
            segments,
            sections,
        })
    }

    pub fn read<R: Read + Seek>(reader: R) -> Result<Self, Error> {
        let elf = Self::read_unchecked(reader)?;
        elf.validate()?;
        Ok(elf)
    }

    pub fn write<W: Write + Seek>(mut self, mut writer: W) -> Result<(), Error> {
        self.finish(&mut writer)?;
        self.validate()?;
        self.header.write(&mut writer)?;
        self.segments.write(&mut writer, &self.header)?;
        self.sections.write(&mut writer, &self.header)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.header.validate()?;
        self.segments.validate(&self.header)?;
        self.sections.validate(&self.header, &self.segments)?;
        Ok(())
    }

    fn finish<W: Write + Seek>(&mut self, mut writer: W) -> Result<(), Error> {
        // Remove old program header.
        if let Some(i) = self
            .segments
            .iter()
            .position(|segment| segment.kind == SegmentKind::ProgramHeader)
        {
            self.free_segment(&mut writer, i)?;
        }
        // Allocate new program header.
        let program_header_len = (self.segments.len() as u64)
            // +1 because PHDR is also a segment
            // +1 because PHDR segment has to be covered by LOAD segment
            .checked_add(2)
            .ok_or(Error::TooBig("No. of segments"))?
            .checked_mul(self.header.class.segment_len() as u64)
            .ok_or(Error::TooBig("No. of segments"))?;
        let phdr_segment_index = self.alloc_segment(Segment {
            kind: SegmentKind::ProgramHeader,
            flags: SegmentFlags::READABLE,
            virtual_address: 0,
            physical_address: 0,
            offset: 0,
            file_size: program_header_len,
            memory_size: program_header_len,
            align: PAGE_SIZE as u64,
        })?;
        let phdr = &self.segments[phdr_segment_index];
        // Allocate LOAD segment to cover PHDR.
        let load = Segment {
            kind: SegmentKind::Loadable,
            flags: SegmentFlags::READABLE,
            virtual_address: phdr.virtual_address,
            physical_address: phdr.physical_address,
            offset: phdr.offset,
            file_size: phdr.file_size,
            memory_size: phdr.memory_size,
            align: phdr.align,
        };
        self.segments.push(load);
        // Allocate new section header.
        self.sections.finish();
        let section_header_len = (self.sections.len() as u64)
            .checked_mul(self.header.class.section_len() as u64)
            .ok_or(Error::TooBig("No. of sections"))?;
        let section_header_offset = self
            .alloc_section_header(section_header_len)
            .ok_or(Error::FileBlockAlloc)?;
        // Update ELF header.
        let phdr = &self.segments[phdr_segment_index];
        self.header.program_header_offset = phdr.offset;
        self.header.num_segments = self.segments.len().try_into().unwrap_or(u16::MAX);
        self.header.section_header_offset = section_header_offset;
        self.header.num_sections = self.sections.len().try_into().unwrap_or(0);
        // Update pseudo-section.
        self.sections[0].info = if self.header.num_segments == u16::MAX {
            self.segments
                .len()
                .try_into()
                .map_err(|_| Error::TooBig("No. of segments"))?
        } else {
            0
        };
        self.sections[0].size = if self.header.num_sections == 0 {
            self.sections
                .len()
                .try_into()
                .map_err(|_| Error::TooBig("No. of sections"))?
        } else {
            0
        };
        self.segments.finish();
        Ok(())
    }

    pub fn read_section_names<F: Read + Seek>(
        &self,
        mut file: F,
    ) -> Result<StringTable, Error> {
        let section = self.sections.get(self.header.section_names_index as usize);
        if let Some(section) = section {
            Ok(section.read_content(&mut file)?.into())
        } else {
            Ok(Default::default())
        }
    }

    pub fn remove_interpreter<F: Read + Write + Seek>(&mut self, mut file: F) -> Result<(), Error> {
        let names = self.read_section_names(&mut file)?;
        // Remove `.interp` section.
        let interpreter_section_index = self.sections.iter().position(|section| {
            if section.kind != SectionKind::ProgramData {
                return false;
            }
            let string = names.get_string(section.name_offset as usize);
            Some(INTERP_SECTION) == string
        });
        if let Some(i) = interpreter_section_index {
            // `INTERP` segment is removed autoamtically.
            self.free_section(&mut file, i, &names)?;
        }
        Ok(())
    }

    pub fn set_interpreter<F: Read + Write + Seek>(
        &mut self,
        mut file: F,
        interpreter: &CStr,
    ) -> Result<(), Error> {
        self.remove_interpreter(&mut file)?;
        let interpreter = interpreter.to_bytes_with_nul();
        let mut names = self.read_section_names(&mut file)?;
        let name_offset = self.get_name_offset(&mut file, INTERP_SECTION, &mut names)?;
        let i = self.alloc_section(
            Section {
                name_offset: name_offset
                    .try_into()
                    .map_err(|_| Error::TooBig("Section name offset"))?,
                kind: SectionKind::ProgramData,
                flags: SectionFlags::ALLOC,
                virtual_address: 0,
                offset: 0,
                size: interpreter.len() as u64,
                link: 0,
                info: 0,
                align: 1,
                entry_len: 0,
            },
            &names,
        )?;
        let section = &self.sections[i];
        section.write_out(&mut file, interpreter)?;
        self.segments.push(Segment {
            kind: SegmentKind::Interpreter,
            flags: SegmentFlags::READABLE,
            offset: section.offset,
            virtual_address: section.virtual_address,
            physical_address: section.virtual_address,
            file_size: section.size,
            memory_size: section.size,
            align: section.align,
        });
        // We don't write segment here since the content and the location is the same as in the
        // `.interp`. section.
        Ok(())
    }

    pub fn remove_dynamic<F: Read + Write + Seek>(
        &mut self,
        mut file: F,
        entry_kind: DynamicEntryKind,
    ) -> Result<(), Error> {
        let result1 = match self
            .segments
            .iter()
            .position(|segment| segment.kind == SegmentKind::Dynamic)
        {
            Some(i) => Some((self.segments[i].read_content(&mut file)?, i)),
            None => None,
        };
        let names = self.read_section_names(&mut file)?;
        let result2 = match self.sections.iter().position(|section| {
            Some(DYNAMIC_SECTION) == names.get_string(section.name_offset as usize)
        }) {
            Some(i) => {
                if result1.is_none() {
                    let bytes = self.sections[i].read_content(&mut file)?;
                    Some((bytes, i))
                } else {
                    // No need to read the same data once more.
                    Some((Vec::new(), i))
                }
            }
            None => None,
        };
        let (dynamic_table_bytes, dynamic_segment_index, dynamic_section_index) =
            match (result1, result2) {
                (Some((bytes, i)), Some((_, j))) => (bytes, Some(i), Some(j)),
                (Some((bytes, i)), None) => (bytes, Some(i), None),
                (None, Some((bytes, j))) => (bytes, None, Some(j)),
                // No `.dynamic` section and no DYNAMIC segment.
                (None, None) => return Ok(()),
            };
        let mut dynamic_table = DynamicTable::from_bytes(
            &dynamic_table_bytes,
            self.header.class,
            self.header.byte_order,
        )?;
        dynamic_table.retain(|(kind, _value)| {
            let retain = *kind != entry_kind;
            if !retain {
                log::trace!("Removing dynamic table entry {:?}", entry_kind);
            }
            retain
        });
        let dynamic_table_bytes =
            dynamic_table.to_bytes(self.header.class, self.header.byte_order)?;
        let dynamic_table_len = dynamic_table_bytes.len() as u64;
        match (dynamic_section_index, dynamic_segment_index) {
            (Some(i), _) => {
                let section = &mut self.sections[i];
                section.size = dynamic_table_len;
                section.write_out(&mut file, &dynamic_table_bytes)?;
            }
            (_, Some(i)) => {
                let segment = &mut self.segments[i];
                segment.file_size = dynamic_table_len;
                segment.memory_size = dynamic_table_len;
                segment.write_out(&mut file, &dynamic_table_bytes)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn add_dynamic_c_str<F: Read + Write + Seek>(
        &mut self,
        mut file: F,
        entry_kind: DynamicEntryKind,
        value: &CStr,
    ) -> Result<(), Error> {
        use DynamicEntryKind::*;
        let mut names = self.read_section_names(&mut file)?;
        let (dynamic_table_bytes, old_dynamic_table_virtual_address) =
            match self.sections.iter().position(|section| {
                Some(DYNAMIC_SECTION) == names.get_string(section.name_offset as usize)
            }) {
                Some(i) => {
                    let section = &self.sections[i];
                    let virtual_address = section.virtual_address;
                    let bytes = section.read_content(&mut file)?;
                    self.free_section(&mut file, i, &names)?;
                    (bytes, virtual_address)
                }
                None => {
                    // `.dynamic` section doesn't exits. Try to find DYNAMIC segment.
                    match self
                        .segments
                        .iter()
                        .position(|segment| segment.kind == SegmentKind::Dynamic)
                    {
                        Some(i) => {
                            let segment = &self.segments[i];
                            let virtual_address = segment.virtual_address;
                            let bytes = segment.read_content(&mut file)?;
                            self.free_segment(&mut file, i)?;
                            (bytes, virtual_address)
                        }
                        None => (Vec::new(), 0),
                    }
                }
            };
        let mut dynamic_table = DynamicTable::from_bytes(
            &dynamic_table_bytes,
            self.header.class,
            self.header.byte_order,
        )?;
        log::trace!("Found dynamic table");
        let dynstr_table_index = match dynamic_table
            .iter()
            .find_map(|(kind, value)| (*kind == StringTableAddress).then_some(value))
        {
            Some(addr) => {
                // Find string table by its virtual address.
                self.sections.iter().position(|section| {
                    section.kind == SectionKind::StringTable && section.virtual_address == *addr
                })
            }
            None => {
                // Couldn't find string table's address in the dynamic table.
                // Try to find the string table by section name.
                self.sections.iter().position(|section| {
                    section.kind == SectionKind::StringTable
                        && Some(DYNSTR_SECTION) == names.get_string(section.name_offset as usize)
                })
            }
        };
        let (mut dynstr_table, dynstr_table_index) = match dynstr_table_index {
            Some(i) => {
                let bytes = self.sections[i].read_content(&mut file)?;
                (StringTable::from(bytes), Some(i))
            }
            None => (Default::default(), None),
        };
        log::trace!("Found `.dynstr` table");
        log::trace!("dynstr table index {:?}", dynstr_table_index);
        let (value_offset, dynstr_table_index) = self.get_string_offset(
            &mut file,
            value,
            dynstr_table_index,
            DYNSTR_SECTION,
            &mut dynstr_table,
            &mut names,
        )?;
        log::trace!("dynstr table index {}", dynstr_table_index);
        // Update dynamic table.
        let dynstr_table_section = &self.sections[dynstr_table_index];
        if !self
            .segments
            .is_loadable(dynstr_table_section.file_offsets())
        {
            self.segments.add(Segment {
                kind: SegmentKind::Loadable,
                flags: SegmentFlags::READABLE | SegmentFlags::WRITABLE,
                offset: dynstr_table_section.offset,
                virtual_address: dynstr_table_section.virtual_address,
                physical_address: dynstr_table_section.virtual_address,
                file_size: dynstr_table_section.size,
                memory_size: dynstr_table_section.size,
                // TODO
                align: PAGE_SIZE as u64,
            });
        }
        dynstr_table_section.write_out(&mut file, dynstr_table.as_ref())?;
        log::trace!("Updated `.dynstr` table");
        dynamic_table.set(StringTableAddress, dynstr_table_section.virtual_address);
        dynamic_table.set(StringTableSize, dynstr_table_section.size);
        dynamic_table.set(entry_kind, value_offset as u64);
        let dynamic_table_contents =
            dynamic_table.to_bytes(self.header.class, self.header.byte_order)?;
        let dynamic_table_len = dynamic_table_contents.len() as u64;
        let dynamic_segment_index = self.alloc_segment(Segment {
            kind: SegmentKind::Dynamic,
            flags: SegmentFlags::READABLE | SegmentFlags::WRITABLE,
            virtual_address: 0,
            physical_address: 0,
            offset: 0,
            file_size: dynamic_table_len,
            memory_size: dynamic_table_len,
            // TODO
            align: PAGE_SIZE as u64,
        })?;
        let new_dynamic_table_virtual_address =
            self.segments[dynamic_segment_index].virtual_address;
        self.segments[dynamic_segment_index].write_out(&mut file, &dynamic_table_contents)?;
        if old_dynamic_table_virtual_address != new_dynamic_table_virtual_address {
            log::trace!(
                "Changed memory offset of the DYNAMIC segment from {:#x} to {:#x}",
                old_dynamic_table_virtual_address,
                new_dynamic_table_virtual_address
            );
        }
        // We don't write section here since the content and the location is the same as in the
        // `.dynamic`. segment.
        self.sections.retain(|section| {
            Some(DYNAMIC_SECTION) != names.get_string(section.name_offset as usize)
        });
        let name_offset = self.get_name_offset(&mut file, DYNAMIC_SECTION, &mut names)?;
        let segment = &self.segments[dynamic_segment_index];
        self.sections.add(Section {
            name_offset: name_offset
                .try_into()
                .map_err(|_| Error::TooBig("Section name"))?,
            kind: SectionKind::Dynamic,
            flags: SectionFlags::ALLOC | SectionFlags::WRITE,
            virtual_address: segment.virtual_address,
            offset: segment.offset,
            size: dynamic_table_len,
            link: dynstr_table_index
                .try_into()
                .map_err(|_| Error::TooBig("Section link"))?,
            info: 0,
            // TODO
            align: PAGE_SIZE as u64,
            entry_len: DYNAMIC_ENTRY_LEN,
        });
        let load = Segment {
            kind: SegmentKind::Loadable,
            flags: segment.flags,
            virtual_address: segment.virtual_address,
            physical_address: segment.physical_address,
            offset: segment.offset,
            file_size: segment.file_size,
            memory_size: segment.memory_size,
            align: segment.align,
        };
        self.segments.push(load);
        Ok(())
    }

    fn get_name_offset<F: Read + Write + Seek>(
        &mut self,
        mut file: F,
        name: &CStr,
        names: &mut StringTable,
    ) -> Result<usize, Error> {
        let name_offset = match names.get_offset(name) {
            Some(name_offset) => {
                log::trace!("Found section name {:?} at offset {}", name, name_offset);
                name_offset
            }
            None => {
                self.free_section(&mut file, self.header.section_names_index as usize, names)?;
                let outer_name_offset = names.insert(name);
                log::trace!(
                    "Adding section name {:?} at offset {}",
                    name,
                    outer_name_offset
                );
                let name_offset = match names.get_offset(SHSTRTAB_SECTION) {
                    Some(name_offset) => name_offset,
                    None => {
                        let offset = names.insert(SHSTRTAB_SECTION);
                        log::trace!(
                            "Adding section name {:?} at offset {}",
                            SHSTRTAB_SECTION,
                            offset
                        );
                        offset
                    }
                };
                let i = self.alloc_section(
                    Section {
                        name_offset: name_offset
                            .try_into()
                            .map_err(|_| Error::TooBig("Section name"))?,
                        kind: SectionKind::StringTable,
                        flags: SectionFlags::ALLOC,
                        virtual_address: 0,
                        offset: 0,
                        size: names.len() as u64,
                        link: 0,
                        info: 0,
                        align: 1,
                        entry_len: 0,
                    },
                    names,
                )?;
                self.sections[i].write_out(&mut file, names.as_ref())?;
                self.header.section_names_index = i
                    .try_into()
                    .map_err(|_| Error::TooBig("Section names index"))?;
                outer_name_offset
            }
        };
        Ok(name_offset)
    }

    fn get_string_offset<F: Read + Write + Seek>(
        &mut self,
        mut file: F,
        string: &CStr,
        table_section_index: Option<usize>,
        table_name: &CStr,
        table: &mut StringTable,
        names: &mut StringTable,
    ) -> Result<(usize, usize), Error> {
        let (string_offset, table_section_index) = match table.get_offset(string) {
            Some(string_offset) => {
                log::trace!(
                    "Found string {:?} in {:?} at offset {}",
                    string,
                    table_name,
                    string_offset
                );
                (string_offset, table_section_index.expect("Should be set"))
            }
            None => {
                if let Some(table_section_index) = table_section_index {
                    self.free_section(&mut file, table_section_index, names)?;
                }
                let outer_string_offset = table.insert(string);
                log::trace!(
                    "Adding string {:?} to {:?} at offset {}",
                    string,
                    table_name,
                    outer_string_offset
                );
                let name_offset = self.get_name_offset(&mut file, table_name, names)?;
                let i = self.alloc_section(
                    Section {
                        name_offset: name_offset
                            .try_into()
                            .map_err(|_| Error::TooBig("Section name"))?,
                        kind: SectionKind::StringTable,
                        flags: SectionFlags::ALLOC,
                        virtual_address: 0,
                        offset: 0,
                        size: table.len() as u64,
                        link: 0,
                        info: 0,
                        // TODO
                        align: PAGE_SIZE as u64,
                        entry_len: 0,
                    },
                    names,
                )?;
                self.sections[i].write_out(&mut file, table.as_ref())?;
                (outer_string_offset, i)
            }
        };
        Ok((string_offset, table_section_index))
    }

    fn free_segment<W: Write + Seek>(&mut self, mut writer: W, i: usize) -> Result<(), Error> {
        let segment = self.segments.free(&mut writer, i)?;
        log::trace!(
            "Removing segment [{i}] {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            segment.kind,
            segment.offset,
            segment.offset + segment.file_size,
            segment.virtual_address,
            segment.virtual_address + segment.memory_size
        );
        if segment.kind == SegmentKind::ProgramHeader {
            // Remove the corresponding LOAD segment only if it exactly matches PHDR offset and
            // in-file size.
            let phdr_offset = segment.offset;
            let phdr_file_size = segment.file_size;
            if let Some(j) = self.segments.iter().position(|segment| {
                segment.kind == SegmentKind::Loadable
                    && segment.offset == phdr_offset
                    && segment.file_size == phdr_file_size
            }) {
                // Remove without recursion.
                let segment = self.segments.free(&mut writer, j)?;
                log::trace!(
                    "Removing segment [{j}] {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
                    segment.kind,
                    segment.offset,
                    segment.offset + segment.file_size,
                    segment.virtual_address,
                    segment.virtual_address + segment.memory_size
                );
            }
        }
        Ok(())
    }

    #[allow(unused)]
    fn split_off_sections(&mut self, i: usize) {
        let segment = &self.segments[i];
        let segment_address_range = segment.address_range();
        let segment_kind = segment.kind;
        let segment_flags = segment.flags;
        for section in self.sections.iter() {
            if section.flags.contains(SectionFlags::ALLOC)
                && segment_address_range.contains(&section.virtual_address)
            {
                log::trace!(
                    "Splitting off section: file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
                    section.offset,
                    section.offset + section.size,
                    section.virtual_address,
                    section.virtual_address + section.size
                );
                self.segments.add(Segment {
                    kind: segment_kind,
                    flags: segment_flags,
                    offset: section.offset,
                    virtual_address: section.virtual_address,
                    physical_address: section.virtual_address,
                    file_size: section.size,
                    memory_size: section.size,
                    align: section.align,
                });
            }
        }
    }

    fn alloc_segment(&mut self, mut segment: Segment) -> Result<usize, Error> {
        segment.virtual_address = self
            .alloc_memory_block(segment.memory_size, segment.align)
            .ok_or(Error::MemoryBlockAlloc)?;
        segment.offset = self
            .alloc_file_block(segment.file_size, segment.virtual_address)
            .ok_or(Error::FileBlockAlloc)?;
        segment.physical_address = segment.virtual_address;
        log::trace!(
            "Allocating segment {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            segment.kind,
            segment.offset,
            segment.offset + segment.file_size,
            segment.virtual_address,
            segment.virtual_address + segment.memory_size
        );
        let i = self.segments.add(segment);
        Ok(i)
    }

    fn free_section<W: Write + Seek>(
        &mut self,
        mut writer: W,
        i: usize,
        names: &StringTable,
    ) -> Result<Section, Error> {
        let section = self.sections.free(&mut writer, i)?;
        let name = names
            .get_string(section.name_offset as usize)
            .unwrap_or_default();
        log::trace!(
            "Removing section [{i}] {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            name,
            section.offset,
            section.offset + section.size,
            section.virtual_address,
            section.virtual_address + section.size
        );
        // Free the corresponding similarly named segment if any.
        if name == INTERP_SECTION {
            if let Some(i) = self
                .segments
                .iter()
                .position(|segment| segment.kind == SegmentKind::Interpreter)
            {
                self.free_segment(&mut writer, i)?;
            }
        }
        if name == DYNAMIC_SECTION {
            if let Some(i) = self
                .segments
                .iter()
                .position(|segment| segment.kind == SegmentKind::Dynamic)
            {
                self.free_segment(&mut writer, i)?;
            }
        }
        // Adjust the size of the corresponding LOAD segment of ALLOC section if any.
        if section.flags.contains(SectionFlags::ALLOC) {
            if let Some(i) = self.segments.iter().position(|segment| {
                segment.kind == SegmentKind::Loadable
                    && segment.contains_virtual_address(section.virtual_address)
            }) {
                // Move every other section in this segment to a separate segment.
                let segment = &self.segments[i];
                let segment_address_range = segment.address_range();
                let segment_kind = segment.kind;
                let segment_flags = segment.flags;
                for section in self.sections.iter() {
                    if section.flags.contains(SectionFlags::ALLOC)
                        && segment_address_range.contains(&section.virtual_address)
                    {
                        log::trace!("Splitting off section {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}", 
                            names.get_string(section.name_offset as usize).unwrap_or_default(),
                            section.offset,
                            section.offset + section.size,
                            section.virtual_address,
                            section.virtual_address + section.size
                        );
                        self.segments.add(Segment {
                            kind: segment_kind,
                            flags: segment_flags,
                            offset: section.offset,
                            virtual_address: section.virtual_address,
                            physical_address: section.virtual_address,
                            file_size: section.size,
                            memory_size: section.size,
                            align: section.align,
                        });
                    }
                }
                // Remove the segment without clearing out its contents.
                self.segments.remove(i);
            }
        }
        Ok(section)
    }

    fn alloc_section(&mut self, mut section: Section, names: &StringTable) -> Result<usize, Error> {
        section.virtual_address = self
            .alloc_memory_block(section.size, section.align)
            .ok_or(Error::MemoryBlockAlloc)?;
        section.offset = self
            .alloc_file_block(section.size, section.virtual_address)
            .ok_or(Error::FileBlockAlloc)?;
        let i = self.sections.add(section);
        let section = &self.sections[i];
        log::trace!(
            "Adding section [{i}] {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            names
                .get_string(section.name_offset as usize)
                .unwrap_or_default(),
            section.offset,
            section.offset + section.size,
            section.virtual_address,
            section.virtual_address + section.size
        );
        Ok(i)
    }

    fn alloc_file_block(&self, size: u64, memory_offset: u64) -> Option<u64> {
        let allocations = self.get_file_allocations();
        allocations.alloc_file_block(size, memory_offset)
    }

    fn alloc_section_header(&self, size: u64) -> Option<u64> {
        let allocations = self.get_file_allocations();
        allocations.alloc_memory_block(size, PAGE_SIZE as u64)
    }

    fn get_file_allocations(&self) -> Allocations {
        let mut allocations = Allocations::new();
        allocations.push(0, self.header.len as u64);
        allocations.extend(
            self.sections
                .iter()
                .filter(|section| matches!(section.kind, SectionKind::NoBits | SectionKind::Null))
                .map(|section| (section.offset, section.offset + section.size)),
        );
        allocations.extend(
            self.segments
                .iter()
                .map(|segment| (segment.offset, segment.offset + segment.file_size)),
        );
        allocations.finish();
        allocations
    }

    fn alloc_memory_block(&self, size: u64, align: u64) -> Option<u64> {
        let mut allocations = Allocations::new();
        allocations.push(0, self.header.len as u64);
        allocations.extend(
            self.sections
                .iter()
                .filter(|section| matches!(section.kind, SectionKind::Null))
                .map(|section| {
                    (
                        section.virtual_address,
                        section.virtual_address + section.size,
                    )
                }),
        );
        allocations.extend(self.segments.iter().map(|segment| {
            (
                segment.virtual_address,
                segment.virtual_address + segment.memory_size,
            )
        }));
        allocations.finish();
        allocations.alloc_memory_block(size, align)
    }
}
