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
        self.header.num_sections = self
            .sections
            .len()
            .try_into()
            .map_err(|_| Error::TooBig("No. of sections"))?;
        self.segments.finish();
        Ok(())
    }

    pub fn read_section_names<F: Read + Write + Seek>(
        &mut self,
        mut file: F,
    ) -> Result<Vec<u8>, Error> {
        let section = self.sections.get(self.header.section_names_index as usize);
        if let Some(section) = section {
            section.read_content(&mut file)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn remove_interpreter<F: Read + Write + Seek>(&mut self, mut file: F) -> Result<(), Error> {
        let names = self.read_section_names(&mut file)?;
        // Free existing `.interp` section if any.
        let interpreter_section_index = self.sections.iter().position(|section| {
            if section.kind != SectionKind::ProgramData {
                return false;
            }
            let c_str_bytes = names.get(section.name_offset as usize..).unwrap_or(&[]);
            Ok(INTERP_SECTION) == CStr::from_bytes_until_nul(c_str_bytes)
        });
        if let Some(i) = interpreter_section_index {
            self.free_section(&mut file, i, &names)?;
        }
        // Free existing `INTERP` segment if any.
        let interp_segment_index = self
            .segments
            .iter()
            .position(|segment| segment.kind == SegmentKind::Interpreter);
        if let Some(i) = interp_segment_index {
            self.free_segment(&mut file, i)?;
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
        self.header.num_segments += 1;
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
            Some(DYNAMIC_SECTION) == get_name(&names, section.name_offset as usize)
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
        let dynamic_table_bytes = match self.sections.iter().position(|section| {
            Some(DYNAMIC_SECTION) == get_name(&names, section.name_offset as usize)
        }) {
            Some(i) => {
                let bytes = self.sections[i].read_content(&mut file)?;
                self.free_section(&mut file, i, &names)?;
                bytes
            }
            None => Vec::new(),
        };
        let mut dynamic_table = DynamicTable::from_bytes(
            &dynamic_table_bytes,
            self.header.class,
            self.header.byte_order,
        )?;
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
                        && Some(DYNSTR_SECTION) == get_name(&names, section.name_offset as usize)
                })
            }
        };
        let (mut dynstr_table, dynstr_table_index) = match dynstr_table_index {
            Some(i) => {
                let bytes = self.sections[i].read_content(&mut file)?;
                (bytes, Some(i))
            }
            None => (Vec::new(), None),
        };
        let (value_offset, dynstr_table_index) = self.get_string_offset(
            &mut file,
            value,
            dynstr_table_index,
            DYNSTR_SECTION,
            &mut dynstr_table,
            &mut names,
        )?;
        // Update dynamic table.
        let dynstr_table_section = &self.sections[dynstr_table_index];
        eprintln!("dynstr = {:?}", dynstr_table_section);
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
        self.header.num_segments = self
            .segments
            .len()
            .try_into()
            .map_err(|_| Error::TooBig("No. of segments"))?;
        dynamic_table.retain(|(kind, _)| {
            let retain = !matches!(kind, StringTableAddress | StringTableSize);
            if !retain {
                log::trace!("Removing dynamic table entry {:?}", kind);
            }
            retain
        });
        log::trace!(
            "Add dynamic table entry: {:?} = {:#x}",
            StringTableAddress,
            dynstr_table_section.virtual_address
        );
        log::trace!(
            "Add dynamic table entry: {:?} = {}",
            StringTableSize,
            dynstr_table_section.size
        );
        log::trace!("Add dynamic table entry: {:?} = {:?}", entry_kind, value);
        dynamic_table.push((StringTableAddress, dynstr_table_section.virtual_address));
        dynamic_table.push((StringTableSize, dynstr_table_section.size));
        dynamic_table.push((entry_kind, value_offset as u64));
        let dynamic_table_contents =
            dynamic_table.to_bytes(self.header.class, self.header.byte_order)?;
        let size = dynamic_table_contents.len() as u64;
        let dynamic_segment_index = self.alloc_segment(Segment {
            kind: SegmentKind::Dynamic,
            flags: SegmentFlags::READABLE | SegmentFlags::WRITABLE,
            virtual_address: 0,
            physical_address: 0,
            offset: 0,
            file_size: size,
            memory_size: size,
            align: DYNAMIC_ALIGN,
        })?;
        self.segments[dynamic_segment_index].write_out(&mut file, &dynamic_table_contents)?;
        // We don't write section here since the content and the location is the same as in the
        // `.dynamic`. segment.
        self.sections.retain(|section| {
            Some(DYNAMIC_SECTION) != get_name(&names, section.name_offset as usize)
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
            size,
            link: dynstr_table_index
                .try_into()
                .map_err(|_| Error::TooBig("Section link"))?,
            info: 0,
            align: DYNAMIC_ALIGN,
            entry_len: 0,
        });
        self.header.num_sections = self
            .sections
            .len()
            .try_into()
            .map_err(|_| Error::TooBig("No. of sections"))?;
        Ok(())
    }

    fn get_name_offset<F: Read + Write + Seek>(
        &mut self,
        mut file: F,
        name: &CStr,
        names: &mut Vec<u8>,
    ) -> Result<usize, Error> {
        let name_offset = match find_name(&names, name.to_bytes_with_nul()) {
            Some(name_offset) => {
                log::trace!("Found section name {:?} at offset {}", name, name_offset);
                name_offset
            }
            None => {
                self.free_section(&mut file, self.header.section_names_index as usize, &names)?;
                if names.is_empty() {
                    // String tables always start with NUL byte.
                    names.push(0);
                }
                let outer_name_offset = names.len();
                log::trace!(
                    "Adding section name {:?} at offset {}",
                    name,
                    outer_name_offset
                );
                names.extend_from_slice(name.to_bytes_with_nul());
                let name_offset = match find_name(&names, SHSTRTAB_SECTION.to_bytes_with_nul()) {
                    Some(name_offset) => name_offset,
                    None => {
                        let offset = names.len();
                        log::trace!(
                            "Adding section name {:?} at offset {}",
                            SHSTRTAB_SECTION,
                            offset
                        );
                        names.extend_from_slice(SHSTRTAB_SECTION.to_bytes_with_nul());
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
                    &names,
                )?;
                self.sections[i].write_out(&mut file, &names)?;
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
        table: &mut Vec<u8>,
        names: &mut Vec<u8>,
    ) -> Result<(usize, usize), Error> {
        let (string_offset, table_section_index) =
            match find_name(&table, string.to_bytes_with_nul()) {
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
                        self.free_section(&mut file, table_section_index, &names)?;
                    }
                    if table.is_empty() {
                        // String tables always start with NUL byte.
                        table.push(0);
                    }
                    let outer_string_offset = table.len();
                    log::trace!(
                        "Adding string {:?} to {:?} at offset {}",
                        string,
                        table_name,
                        outer_string_offset
                    );
                    table.extend_from_slice(string.to_bytes_with_nul());
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
                            align: 1,
                            entry_len: 0,
                        },
                        &names,
                    )?;
                    self.sections[i].write_out(&mut file, &table)?;
                    self.header.section_names_index = i
                        .try_into()
                        .map_err(|_| Error::TooBig("Section names index"))?;
                    (outer_string_offset, i)
                }
            };
        Ok((string_offset, table_section_index))
    }

    fn free_segment<W: Write + Seek>(&mut self, mut writer: W, i: usize) -> Result<(), Error> {
        let segment = self.segments.free(&mut writer, i)?;
        log::trace!(
            "Removing segment {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            segment.kind,
            segment.offset,
            segment.offset + segment.file_size,
            segment.virtual_address,
            segment.virtual_address + segment.memory_size
        );
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
        self.header.num_segments = self
            .segments
            .len()
            .try_into()
            .map_err(|_| Error::TooBig("No. of segments"))?;
        Ok(i)
    }

    fn free_section<W: Write + Seek>(
        &mut self,
        mut writer: W,
        i: usize,
        names: &[u8],
    ) -> Result<Section, Error> {
        let section = self.sections.free(&mut writer, i)?;
        let c_str_bytes = names.get(section.name_offset as usize..).unwrap_or(&[]);
        let name = CStr::from_bytes_until_nul(c_str_bytes).unwrap_or_default();
        log::trace!(
            "Removing section {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
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
                // TODO always split?
                let segment_address_range = segment.address_range();
                let segment_kind = segment.kind;
                let segment_flags = segment.flags;
                for section in self.sections.iter() {
                    if section.flags.contains(SectionFlags::ALLOC)
                        && segment_address_range.contains(&section.virtual_address)
                    {
                        log::trace!("Splitting off section {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}", 
                            get_name(names, section.name_offset as usize).unwrap_or_default(),
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
                self.free_segment(&mut writer, i)?;
            }
        }
        Ok(section)
    }

    fn alloc_section(&mut self, mut section: Section, names: &[u8]) -> Result<usize, Error> {
        section.virtual_address = self
            .alloc_memory_block(section.size, section.align)
            .ok_or(Error::MemoryBlockAlloc)?;
        section.offset = self
            .alloc_file_block(section.size, section.virtual_address)
            .ok_or(Error::FileBlockAlloc)?;
        log::trace!(
            "Adding section {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            get_name(names, section.name_offset as usize).unwrap_or_default(),
            section.offset,
            section.offset + section.size,
            section.virtual_address,
            section.virtual_address + section.size
        );
        let i = self.sections.add(section);
        self.header.num_sections = self
            .sections
            .len()
            .try_into()
            .map_err(|_| Error::TooBig("No. of sections"))?;
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
        // TODO this is wrong!
        //allocations.push(
        //    self.header.program_header_offset,
        //    self.header.program_header_offset
        //        + self.header.segment_len as u64 * self.header.num_segments as u64,
        //);
        //// TODO this is wrong!
        //allocations.push(
        //    self.header.section_header_offset,
        //    self.header.section_header_offset
        //        + self.header.section_len as u64 * self.header.num_sections as u64,
        //);
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

fn find_name(names: &[u8], name: &[u8]) -> Option<usize> {
    if name.is_empty() {
        return Some(0);
    }
    if names.is_empty() {
        return None;
    }
    let mut j = 0;
    let n = name.len();
    for i in 0..names.len() {
        if names[i] == name[j] {
            j += 1;
            if j == n {
                return Some(i + 1 - n);
            }
        } else {
            j = 0;
        }
    }
    None
}

fn get_name(names: &[u8], offset: usize) -> Option<&CStr> {
    let Some(c_str_bytes) = names.get(offset..) else {
        return None;
    };
    CStr::from_bytes_until_nul(c_str_bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Segment;
    use crate::SegmentFlags;
    use crate::SegmentKind;
    use arbtest::arbtest;
    use std::fs::OpenOptions;

    #[test]
    fn test_find_name() {
        assert_eq!(Some(0), find_name(b"hello\0", b"hello\0"));
        assert_eq!(Some(1), find_name(b"\0hello\0", b"hello\0"));
        assert_eq!(Some(7), find_name(b"\0first\0hello\0", b"hello\0"));
        assert_eq!(None, find_name(b"", b"hello\0"));
        assert_eq!(Some(0), find_name(b"", b""));
        assert_eq!(Some(0), find_name(b"123", b""));
    }
}
