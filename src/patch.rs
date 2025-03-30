use alloc::ffi::CString;
use alloc::vec::Vec;
use core::ffi::CStr;
use log::log_enabled;
use log::Level;

use crate::constants::*;
use crate::BlockRead;
use crate::BlockWrite;
use crate::DynamicTable;
use crate::DynamicTag;
use crate::DynamicValue;
use crate::Elf;
use crate::ElfRead;
use crate::ElfSeek;
use crate::ElfWrite;
use crate::Error;
use crate::Section;
use crate::SectionFlags;
use crate::SectionKind;
use crate::Segment;
use crate::SegmentFlags;
use crate::SegmentKind;
use crate::SpaceAllocator;
use crate::StringTable;
use crate::SymbolTable;

/// ELF patcher.
///
/// Supports modifying the interpreter and RPATH/RUNPATH.
pub struct ElfPatcher<F> {
    elf: Elf,
    file: F,
    page_size: u64,
    /// Section names.
    names: Option<StringTable>,
}

impl<F: ElfRead + ElfWrite + ElfSeek> ElfPatcher<F> {
    /// Create new patcher from [`Elf`] and file.
    ///
    /// The file should be open for writing.
    pub fn new(elf: Elf, file: F) -> Self {
        Self {
            elf,
            file,
            page_size: DEFAULT_PAGE_SIZE,
            names: None,
        }
    }

    /// Change page size.
    ///
    /// Page size is used during validation and to allocate space for new sections and segments.
    pub fn set_page_size(&mut self, value: u64) {
        self.page_size = value;
    }

    /// Get the current ELF.
    pub fn elf(&self) -> &Elf {
        &self.elf
    }

    /// Convert into underlying reperesentation.
    pub fn into_inner(self) -> (Elf, F) {
        (self.elf, self.file)
    }

    /// Finish and write the current ELF to the file.
    ///
    /// Before writing this method generates new program header, new section header and validates them.
    pub fn finish(mut self) -> Result<F, Error> {
        self.do_finish()?;
        self.elf.write(&mut self.file)?;
        Ok(self.file)
    }

    fn do_finish(&mut self) -> Result<(), Error> {
        // Remove old program header.
        if let Some(i) = self
            .elf
            .segments
            .iter()
            .position(|segment| segment.kind == SegmentKind::ProgramHeader)
        {
            self.free_segment(i)?;
        }
        // Allocate new program header.
        let program_header_len = (self.elf.segments.len() as u64)
            // +1 because PHDR is also a segment
            // +1 because PHDR segment has to be covered by LOAD segment
            .checked_add(2)
            .ok_or(Error::TooBig("No. of segments"))?
            .checked_mul(self.elf.header.class.segment_len() as u64)
            .ok_or(Error::TooBig("No. of segments"))?;
        let phdr_segment_index = self.alloc_segment(Segment {
            kind: SegmentKind::ProgramHeader,
            flags: SegmentFlags::READABLE,
            virtual_address: 0,
            physical_address: 0,
            offset: 0,
            file_size: program_header_len,
            memory_size: program_header_len,
            align: PHDR_ALIGN,
        })?;
        // Allocate new section header.
        self.elf.sections.finish();
        let section_header_len = (self.elf.sections.len() as u64)
            .checked_mul(self.elf.header.class.section_len() as u64)
            .ok_or(Error::TooBig("No. of sections"))?;
        let section_header_offset = self
            .alloc_section_header(section_header_len)
            .ok_or(Error::FileSpaceAlloc)?;
        // Update ELF header.
        let phdr = &self.elf.segments[phdr_segment_index];
        self.elf.header.program_header_offset = phdr.offset;
        self.elf.header.num_segments = self.elf.segments.len().try_into().unwrap_or(u16::MAX);
        self.elf.header.section_header_offset = section_header_offset;
        self.elf.header.num_sections = self.elf.sections.len().try_into().unwrap_or(0);
        // Update pseudo-section.
        self.elf.sections[0].info = if self.elf.header.num_segments == u16::MAX {
            self.elf
                .segments
                .len()
                .try_into()
                .map_err(|_| Error::TooBig("No. of segments"))?
        } else {
            0
        };
        self.elf.sections[0].size = if self.elf.header.num_sections == 0 {
            self.elf
                .sections
                .len()
                .try_into()
                .map_err(|_| Error::TooBig("No. of sections"))?
        } else {
            0
        };
        self.elf.segments.finish();
        Ok(())
    }

    /// Get the interpreter.
    pub fn read_interpreter(&mut self) -> Result<Option<CString>, Error> {
        let Some(interp) = self.read_section(INTERP_SECTION)? else {
            return Ok(None);
        };
        Ok(Some(CString::from_vec_with_nul(interp)?))
    }

    /// Remove the interpreter.
    ///
    /// Removes all `.interp` sections and `INTERP` segments.
    pub fn remove_interpreter(&mut self) -> Result<(), Error> {
        let names = get_section_names!(self);
        // Remove all `.interp` sections.
        let n = self.elf.sections.len();
        for i in 0..n {
            let name_offset = self.elf.sections[i].name_offset as usize;
            if Some(INTERP_SECTION) != names.get_string(name_offset) {
                continue;
            }
            self.elf.sections.free(&mut self.file, i)?;
        }
        // Clear the contents of all `INTERP` segments first.
        for segment in self.elf.segments.iter() {
            if segment.kind != SegmentKind::Interpreter {
                continue;
            }
            segment.clear_content(&mut self.file)?;
        }
        // Remove all `INTERP` segments.
        self.elf
            .segments
            .retain(|segment| segment.kind != SegmentKind::Interpreter);
        Ok(())
    }

    /// Set the interpreter.
    ///
    /// Adds or modifies `.interp` section and `INTERP` segment.
    pub fn set_interpreter(&mut self, interpreter: &CStr) -> Result<(), Error> {
        self.remove_interpreter()?;
        let name_offset = self.get_name_offset(INTERP_SECTION)?;
        // Add `.interp` section and overlay it with LOAD segment.
        let i = self.alloc_section(Section {
            name_offset: name_offset
                .try_into()
                .map_err(|_| Error::TooBig("Section name offset"))?,
            kind: SectionKind::ProgramBits,
            flags: SectionFlags::ALLOC,
            virtual_address: 0,
            offset: 0,
            size: (interpreter.count_bytes() + 1) as u64,
            link: 0,
            info: 0,
            align: INTERP_ALIGN,
            entry_len: 0,
        })?;
        let section = &self.elf.sections[i];
        section.write_content(
            &mut self.file,
            self.elf.header.class,
            self.elf.header.byte_order,
            interpreter,
        )?;
        // Add INTERP segment.
        self.elf.segments.push(Segment {
            kind: SegmentKind::Interpreter,
            flags: SegmentFlags::READABLE,
            offset: section.offset,
            virtual_address: section.virtual_address,
            physical_address: section.virtual_address,
            file_size: section.size,
            memory_size: section.size,
            align: section.align,
        });
        Ok(())
    }

    /// Remove all entries for the specified dynamic tag from the dynamic table.
    pub fn remove_dynamic_tag(&mut self, tag: DynamicTag) -> Result<(), Error> {
        let Some(i) = self
            .elf
            .sections
            .iter()
            .position(|section| section.kind == SectionKind::Dynamic)
        else {
            return Ok(());
        };
        let section = &self.elf.sections[i];
        self.file.seek(section.offset)?;
        let mut table = DynamicTable::read(
            &mut self.file,
            self.elf.header.class,
            self.elf.header.byte_order,
            section.size,
        )?;
        table.retain(|(kind, _value)| {
            let retain = *kind != tag;
            if !retain {
                log::trace!("Removing dynamic table entry {:?}", tag);
            }
            retain
        });
        // Update DYNAMIC section.
        let table_len = table.in_file_len(self.elf.header.class) as u64;
        let section = &mut self.elf.sections[i];
        section.size = table_len;
        self.file.seek(section.offset)?;
        table.write(
            &mut self.file,
            self.elf.header.class,
            self.elf.header.byte_order,
        )?;
        // Update DYNAMIC segment.
        let Some(i) = self
            .elf
            .segments
            .iter()
            .position(|segment| segment.kind == SegmentKind::Dynamic)
        else {
            return Ok(());
        };
        let segment = &mut self.elf.segments[i];
        segment.file_size = table_len;
        segment.memory_size = table_len;
        Ok(())
    }

    /// Read dynamic table.
    pub fn read_dynamic_table(&mut self) -> Result<Option<DynamicTable>, Error> {
        let Some(i) = self
            .elf
            .sections
            .iter()
            .position(|section| section.kind == SectionKind::Dynamic)
        else {
            return Ok(None);
        };
        let section = &self.elf.sections[i];
        self.file.seek(section.offset)?;
        let table = DynamicTable::read(
            &mut self.file,
            self.elf.header.class,
            self.elf.header.byte_order,
            section.size,
        )?;
        Ok(Some(table))
    }

    /// Read dynamic string table.
    pub fn read_dynamic_string_table(&mut self) -> Result<StringTable, Error> {
        let names = self
            .elf
            .read_section_names(&mut self.file)?
            .unwrap_or_default();
        let table = match self.elf.sections.iter().position(|section| {
            Some(DYNSTR_SECTION) == names.get_string(section.name_offset as usize)
        }) {
            Some(i) => self.elf.sections[i].read_content(
                &mut self.file,
                self.elf.header.class,
                self.elf.header.byte_order,
            )?,
            None => Default::default(),
        };
        Ok(table)
    }

    /// Set the value under the specified dynamic tag in the dynamic table.
    ///
    /// Does nothing if the table is not present in the file.
    pub fn set_library_search_path<'a>(
        &mut self,
        entry_kind: DynamicTag,
        value: impl Into<DynamicValue<'a>>,
    ) -> Result<(), Error> {
        use DynamicTag::*;
        assert!(matches!(entry_kind, Rpath | Runpath));
        // Read and remove dynamic table.
        let (mut dynamic_table, old_dynamic_table_virtual_address) = match self
            .elf
            .sections
            .iter()
            .position(|section| section.kind == SectionKind::Dynamic)
        {
            Some(i) => {
                let section = &self.elf.sections[i];
                let virtual_address = section.virtual_address;
                self.file.seek(section.offset)?;
                let dynamic_table = DynamicTable::read(
                    &mut self.file,
                    self.elf.header.class,
                    self.elf.header.byte_order,
                    section.size,
                )?;
                self.free_section(i, DYNAMIC_SECTION)?;
                (dynamic_table, virtual_address)
            }
            None => {
                log::trace!("Couldn't find DYNAMIC section");
                return Ok(());
            }
        };
        // Update `.dynstr` table.
        let dynstr_table_index = {
            let dynstr_table_index = match dynamic_table.get(StringTableAddress) {
                Some(addr) => {
                    // Find string table by its virtual address.
                    self.elf.sections.iter().position(|section| {
                        section.kind == SectionKind::StringTable && section.virtual_address == addr
                    })
                }
                None => {
                    // Couldn't find string table's address in the dynamic table.
                    // Try to find the string table by section name.
                    let names = get_section_names!(self);
                    self.elf.sections.iter().position(|section| {
                        section.kind == SectionKind::StringTable
                            && Some(DYNSTR_SECTION)
                                == names.get_string(section.name_offset as usize)
                    })
                }
            };
            let Some(dynstr_table_index) = dynstr_table_index else {
                log::trace!("Couldn't find `.dynstr` section");
                return Ok(());
            };
            let mut dynstr_table: StringTable = self.elf.sections[dynstr_table_index]
                .read_content(
                    &mut self.file,
                    self.elf.header.class,
                    self.elf.header.byte_order,
                )?;
            let (value, dynstr_table_index) = match value.into() {
                DynamicValue::CStr(value) => {
                    let (offset, i) = self.get_string_offset(
                        value,
                        Some(dynstr_table_index),
                        DYNSTR_SECTION,
                        &mut dynstr_table,
                    )?;
                    (offset as u64, i)
                }
                DynamicValue::Word(value) => (value, dynstr_table_index),
            };
            // Write `.dynstr` section.
            let dynstr_table_section = &self.elf.sections[dynstr_table_index];
            dynstr_table_section.write_content(
                &mut self.file,
                self.elf.header.class,
                self.elf.header.byte_order,
                &dynstr_table,
            )?;
            // Update dynamic table.
            dynamic_table.retain(|(kind, _value)| {
                let retain = !matches!(kind, Rpath | Runpath);
                if !retain {
                    log::trace!("Removing dynamic table entry {:?}", kind);
                }
                retain
            });
            dynamic_table.set(StringTableAddress, dynstr_table_section.virtual_address);
            dynamic_table.set(StringTableSize, dynstr_table_section.size);
            dynamic_table.set(entry_kind, value);
            log::trace!("Updated `.dynstr` table");
            dynstr_table_index
        };
        // Update dynamic table.
        let new_dynamic_table_virtual_address = {
            let dynamic_table_len = dynamic_table.in_file_len(self.elf.header.class) as u64;
            let name_offset = self.get_name_offset(DYNAMIC_SECTION)?;
            let dynamic_section_index = self.alloc_section(Section {
                name_offset: name_offset
                    .try_into()
                    .map_err(|_| Error::TooBig("Section name"))?,
                kind: SectionKind::Dynamic,
                flags: SectionFlags::ALLOC | SectionFlags::WRITE,
                virtual_address: 0,
                offset: 0,
                size: dynamic_table_len,
                link: dynstr_table_index
                    .try_into()
                    .map_err(|_| Error::TooBig("Section link"))?,
                info: 0,
                align: DYNAMIC_ALIGN,
                entry_len: DYNAMIC_ENTRY_LEN,
            })?;
            let new_dynamic_table_virtual_address =
                self.elf.sections[dynamic_section_index].virtual_address;
            {
                let section = &self.elf.sections[dynamic_section_index];
                self.file.seek(section.offset)?;
                dynamic_table.write(
                    &mut self.file,
                    self.elf.header.class,
                    self.elf.header.byte_order,
                )?;
                self.elf.segments.push(Segment {
                    kind: SegmentKind::Dynamic,
                    flags: SegmentFlags::READABLE | SegmentFlags::WRITABLE,
                    offset: section.offset,
                    virtual_address: section.virtual_address,
                    physical_address: section.virtual_address,
                    file_size: section.size,
                    memory_size: section.size,
                    align: section.align,
                });
            }
            if old_dynamic_table_virtual_address != new_dynamic_table_virtual_address {
                log::trace!(
                    "Changed memory offset of the DYNAMIC segment from {:#x} to {:#x}",
                    old_dynamic_table_virtual_address,
                    new_dynamic_table_virtual_address
                );
            }
            log::trace!("Updated DYNAMIC section");
            new_dynamic_table_virtual_address
        };
        // Update symbol tables.
        for section in self.elf.sections.iter_mut() {
            if !matches!(
                section.kind,
                SectionKind::SymbolTable | SectionKind::DynamicSymbolTable
            ) {
                continue;
            }
            self.file.seek(section.offset)?;
            let mut symbol_table = SymbolTable::read(
                &mut self.file,
                self.elf.header.class,
                self.elf.header.byte_order,
                section.size,
            )?;
            let mut changed = false;
            for symbol in symbol_table.iter_mut() {
                if symbol.address == old_dynamic_table_virtual_address {
                    log::trace!(
                        "Changed dynamic table address from {:#x} to {:#x} in {:?}",
                        symbol.address,
                        new_dynamic_table_virtual_address,
                        section.kind
                    );
                    symbol.address = new_dynamic_table_virtual_address;
                    changed = true;
                }
            }
            if changed {
                self.file.seek(section.offset)?;
                symbol_table.write(
                    &mut self.file,
                    self.elf.header.class,
                    self.elf.header.byte_order,
                )?;
            }
        }
        Ok(())
    }

    fn get_name_offset(&mut self, name: &CStr) -> Result<usize, Error> {
        let names = get_section_names_mut!(self);
        let name_offset = match names.get_offset(name) {
            Some(name_offset) => {
                log::trace!("Found section name {:?} at offset {}", name, name_offset);
                name_offset
            }
            None => {
                self.elf
                    .sections
                    .free(&mut self.file, self.elf.header.section_names_index as usize)?;
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
                let size = names.as_bytes().len() as u64;
                let i = self.alloc_section(Section {
                    name_offset: name_offset
                        .try_into()
                        .map_err(|_| Error::TooBig("Section name"))?,
                    kind: SectionKind::StringTable,
                    flags: SectionFlags::ALLOC,
                    virtual_address: 0,
                    offset: 0,
                    size,
                    link: 0,
                    info: 0,
                    align: STRING_TABLE_ALIGN,
                    entry_len: 0,
                })?;
                let names = get_section_names!(self);
                self.elf.sections[i].write_content(
                    &mut self.file,
                    self.elf.header.class,
                    self.elf.header.byte_order,
                    &names,
                )?;
                self.elf.header.section_names_index = i
                    .try_into()
                    .map_err(|_| Error::TooBig("Section names index"))?;
                outer_name_offset
            }
        };
        Ok(name_offset)
    }

    fn get_string_offset(
        &mut self,
        string: &CStr,
        table_section_index: Option<usize>,
        table_name: &CStr,
        table: &mut StringTable,
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
                    self.free_section(table_section_index, table_name)?;
                }
                let outer_string_offset = table.insert(string);
                log::trace!(
                    "Adding string {:?} to {:?} at offset {}",
                    string,
                    table_name,
                    outer_string_offset
                );
                let name_offset = self.get_name_offset(table_name)?;
                let i = self.alloc_section(Section {
                    name_offset: name_offset
                        .try_into()
                        .map_err(|_| Error::TooBig("Section name"))?,
                    kind: SectionKind::StringTable,
                    flags: SectionFlags::ALLOC,
                    virtual_address: 0,
                    offset: 0,
                    size: table.as_bytes().len() as u64,
                    link: 0,
                    info: 0,
                    align: STRING_TABLE_ALIGN,
                    entry_len: 0,
                })?;
                self.elf.sections[i].write_content(
                    &mut self.file,
                    self.elf.header.class,
                    self.elf.header.byte_order,
                    &table,
                )?;
                (outer_string_offset, i)
            }
        };
        Ok((string_offset, table_section_index))
    }

    fn free_segment(&mut self, i: usize) -> Result<(), Error> {
        let segment = self.elf.segments.free(&mut self.file, i)?;
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
            if let Some(j) = self.elf.segments.iter().position(|segment| {
                segment.kind == SegmentKind::Loadable
                    && segment.offset == phdr_offset
                    && segment.file_size == phdr_file_size
            }) {
                // Remove without recursion.
                let segment = self.elf.segments.free(&mut self.file, j)?;
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

    fn alloc_segment(&mut self, mut segment: Segment) -> Result<usize, Error> {
        let alloc = SpaceAllocator::new(
            self.elf.header.class,
            self.elf.page_size(),
            &self.elf.sections,
            &mut self.elf.segments,
        );
        alloc.allocate_segment(&mut segment)?;
        /*
        segment.virtual_address = self
            .alloc_memory_block(segment.memory_size, segment.align)
            .ok_or(Error::MemoryBlockAlloc)?;
        segment.offset = self
            .alloc_file_block(segment.file_size, segment.virtual_address)
            .ok_or(Error::FileBlockAlloc)?;
        segment.physical_address = segment.virtual_address;
        */
        log::trace!(
            "Allocating segment {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            segment.kind,
            segment.offset,
            segment.offset + segment.file_size,
            segment.virtual_address,
            segment.virtual_address + segment.memory_size
        );
        let i = self.elf.segments.add(segment);
        Ok(i)
    }

    fn free_section(&mut self, i: usize, name: &CStr) -> Result<Section, Error> {
        let section = self.elf.sections.free(&mut self.file, i)?;
        log::trace!(
            "Removing section [{i}] {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            name,
            section.offset,
            section.offset + section.size,
            section.virtual_address,
            section.virtual_address + section.size
        );
        // Free the corresponding similarly named segment if any.
        if name == DYNAMIC_SECTION {
            if let Some(i) = self
                .elf
                .segments
                .iter()
                .position(|segment| segment.kind == SegmentKind::Dynamic)
            {
                self.free_segment(i)?;
            }
        }
        /*
        // Adjust the size of the corresponding LOAD segment of ALLOC section if any.
        if section.flags.contains(SectionFlags::ALLOC) {
            if let Some(i) = self.segments.iter().position(|segment| {
                segment.kind == SegmentKind::Loadable
                    && segment.contains_virtual_address(section.virtual_address)
            }) {
                // Move every other section in this segment to a separate segment.
                let segment = &self.segments[i];
                let segment_address_range = segment.virtual_address_range();
                let segment_kind = segment.kind;
                let segment_flags = segment.flags;
                let mut new_segments = Vec::new();
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
                        new_segments.push(Segment {
                            kind: segment_kind,
                            flags: segment_flags,
                            offset: section.offset,
                            virtual_address: section.virtual_address,
                            physical_address: section.virtual_address,
                            file_size: section.size,
                            memory_size: section.size,
                            align: self.page_size as u64,
                        });
                    }
                }
                // Remove the segment without clearing out its contents.
                self.segments.remove(i);
                for segment in new_segments.into_iter() {
                    self.alloc_segment(segment)?;
                }
            }
        }
        */
        Ok(section)
    }

    fn alloc_section(&mut self, mut section: Section) -> Result<usize, Error> {
        let alloc = SpaceAllocator::new(
            self.elf.header.class,
            self.elf.page_size(),
            &self.elf.sections,
            &mut self.elf.segments,
        );
        alloc.allocate_section(&mut section)?;
        let i = self.elf.sections.add(section);
        if log_enabled!(Level::Trace) {
            let names = get_section_names!(self);
            let section = &self.elf.sections[i];
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
        }
        Ok(i)
    }

    fn alloc_section_header(&mut self, size: u64) -> Option<u64> {
        let alloc = SpaceAllocator::new(
            self.elf.header.class,
            self.page_size,
            &self.elf.sections,
            &mut self.elf.segments,
        );
        alloc.allocate_file_space(size, SECTION_HEADER_ALIGN)
    }

    /// Get string table that contains section names.
    pub fn get_section_names(&mut self) -> Result<&StringTable, Error> {
        Ok(get_section_names!(self))
    }

    /// Read section that has specified name.
    pub fn read_section(&mut self, name: &CStr) -> Result<Option<Vec<u8>>, Error> {
        let names = get_section_names!(self);
        self.elf.read_section(name, names, &mut self.file)
    }

    fn update_section_names(&mut self) -> Result<(), Error> {
        self.names = Some(
            self.elf
                .read_section_names(&mut self.file)?
                .unwrap_or_default(),
        );
        Ok(())
    }
}

macro_rules! get_section_names {
    ($self: ident) => {{
        if $self.names.is_none() {
            $self.update_section_names()?;
        }
        unsafe { $self.names.as_ref().unwrap_unchecked() }
    }};
}

use get_section_names;

macro_rules! get_section_names_mut {
    ($self: ident) => {{
        if $self.names.is_none() {
            $self.update_section_names()?;
        }
        unsafe { $self.names.as_mut().unwrap_unchecked() }
    }};
}

use get_section_names_mut;
