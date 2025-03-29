use alloc::ffi::CString;
use alloc::vec::Vec;
use core::ffi::CStr;
use core::ops::Deref;

use crate::constants::*;
use crate::BlockIo;
use crate::DynamicTable;
use crate::DynamicTag;
use crate::Elf;
use crate::ElfRead;
use crate::ElfSeek;
use crate::ElfWrite;
use crate::Error;
use crate::RelTable;
use crate::RelaTable;
use crate::Section;
use crate::SectionFlags;
use crate::SectionKind;
use crate::Segment;
use crate::SegmentFlags;
use crate::SegmentKind;
use crate::SpaceAllocator;
use crate::StringTable;
use crate::SymbolTable;

pub struct ElfPatcher<F> {
    elf: Elf,
    file: F,
    page_size: u64,
}

impl<F: ElfRead + ElfWrite + ElfSeek> ElfPatcher<F> {
    pub fn new(elf: Elf, file: F) -> Self {
        Self {
            elf,
            file,
            page_size: DEFAULT_PAGE_SIZE,
        }
    }

    pub fn set_page_size(&mut self, value: u64) {
        self.page_size = value;
    }

    pub fn elf(&self) -> &Elf {
        &self.elf
    }

    pub fn into_inner(self) -> (Elf, F) {
        (self.elf, self.file)
    }

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

    pub fn read_interpreter(&mut self) -> Result<Option<CString>, Error> {
        // TODO use read_section
        let names = self
            .elf
            .read_section_names(&mut self.file)?
            .unwrap_or_default();
        let interpreter_section_index = self.elf.sections.iter().position(|section| {
            if section.kind != SectionKind::ProgramBits {
                return false;
            }
            let string = names.get_string(section.name_offset as usize);
            Some(INTERP_SECTION) == string
        });
        match interpreter_section_index {
            Some(i) => Ok(CString::from_vec_with_nul(
                self.elf.sections[i].read_content(&mut self.file)?,
            )
            .ok()),
            None => Ok(None),
        }
    }

    pub fn remove_interpreter(&mut self) -> Result<(), Error> {
        let names = self
            .elf
            .read_section_names(&mut self.file)?
            .unwrap_or_default();
        // Remove `.interp` section.
        let interpreter_section_index = self.elf.sections.iter().position(|section| {
            if section.kind != SectionKind::ProgramBits {
                return false;
            }
            let string = names.get_string(section.name_offset as usize);
            Some(INTERP_SECTION) == string
        });
        if let Some(i) = interpreter_section_index {
            // `INTERP` segment is removed automatically.
            self.free_section(i, &names)?;
        }
        Ok(())
    }

    pub fn set_interpreter(&mut self, interpreter: &CStr) -> Result<(), Error> {
        self.remove_interpreter()?;
        let interpreter = interpreter.to_bytes_with_nul();
        let mut names = self
            .elf
            .read_section_names(&mut self.file)?
            .unwrap_or_default();
        let name_offset = self.get_name_offset(INTERP_SECTION, &mut names)?;
        let i = self.alloc_section(
            Section {
                name_offset: name_offset
                    .try_into()
                    .map_err(|_| Error::TooBig("Section name offset"))?,
                kind: SectionKind::ProgramBits,
                flags: SectionFlags::ALLOC,
                virtual_address: 0,
                offset: 0,
                size: interpreter.len() as u64,
                link: 0,
                info: 0,
                align: INTERP_ALIGN,
                entry_len: 0,
            },
            &names,
        )?;
        let section = &self.elf.sections[i];
        section.write_out(&mut self.file, interpreter)?;
        let segment = Segment {
            kind: SegmentKind::Interpreter,
            flags: SegmentFlags::READABLE,
            offset: section.offset,
            virtual_address: section.virtual_address,
            physical_address: section.virtual_address,
            file_size: section.size,
            memory_size: section.size,
            align: section.align,
        };
        self.elf.segments.push(segment);
        // We don't write segment here since the content and the location is the same as in the
        // `.interp`. section.
        Ok(())
    }

    pub fn remove_dynamic(&mut self, entry_kind: DynamicTag) -> Result<(), Error> {
        let result1 = match self
            .elf
            .segments
            .iter()
            .position(|segment| segment.kind == SegmentKind::Dynamic)
        {
            Some(i) => Some((self.elf.segments[i].read_content(&mut self.file)?, i)),
            None => None,
        };
        let names = self
            .elf
            .read_section_names(&mut self.file)?
            .unwrap_or_default();
        let result2 = match self.elf.sections.iter().position(|section| {
            Some(DYNAMIC_SECTION) == names.get_string(section.name_offset as usize)
        }) {
            Some(i) => {
                if result1.is_none() {
                    let bytes = self.elf.sections[i].read_content(&mut self.file)?;
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
        let mut dynamic_table = DynamicTable::read(
            &mut &dynamic_table_bytes[..],
            self.elf.header.class,
            self.elf.header.byte_order,
            dynamic_table_bytes.len() as u64,
        )?;
        dynamic_table.retain(|(kind, _value)| {
            let retain = *kind != entry_kind;
            if !retain {
                log::trace!("Removing dynamic table entry {:?}", entry_kind);
            }
            retain
        });
        let dynamic_table_len = dynamic_table.in_file_len(self.elf.header.class) as u64;
        match (dynamic_section_index, dynamic_segment_index) {
            (Some(i), _) => {
                let section = &mut self.elf.sections[i];
                section.size = dynamic_table_len;
                self.file.seek(section.offset)?;
                dynamic_table.write(
                    &mut self.file,
                    self.elf.header.class,
                    self.elf.header.byte_order,
                )?;
            }
            (_, Some(i)) => {
                let segment = &mut self.elf.segments[i];
                segment.file_size = dynamic_table_len;
                segment.memory_size = dynamic_table_len;
                self.file.seek(segment.offset)?;
                dynamic_table.write(
                    &mut self.file,
                    self.elf.header.class,
                    self.elf.header.byte_order,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn read_symbol_table(&mut self) -> Result<Option<(SymbolTable, usize)>, Error> {
        let names = self
            .elf
            .read_section_names(&mut self.file)?
            .unwrap_or_default();
        let Some(i) = self.elf.sections.iter().position(|section| {
            section.kind == SectionKind::SymbolTable
                && Some(SYMTAB_SECTION) == names.get_string(section.name_offset as usize)
        }) else {
            return Ok(None);
        };
        let section = &self.elf.sections[i];
        self.file.seek(section.offset)?;
        let table = SymbolTable::read(
            &mut self.file,
            self.elf.header.class,
            self.elf.header.byte_order,
            section.size,
        )?;
        Ok(Some((table, i)))
    }

    pub fn read_rel_table_for(
        &mut self,
        section_index: u32,
    ) -> Result<Option<(RelTable, usize)>, Error> {
        let Some(i) = self.elf.sections.iter().position(|section| {
            use SectionKind::*;
            matches!(section.kind, RelTable) && section.link == section_index
        }) else {
            return Ok(None);
        };
        let section = &self.elf.sections[i];
        self.file.seek(section.offset)?;
        let table = RelTable::read(
            &mut self.file,
            self.elf.header.class,
            self.elf.header.byte_order,
            section.size,
        )?;
        Ok(Some((table, i)))
    }

    pub fn read_rela_table_for(
        &mut self,
        section_index: u32,
    ) -> Result<Option<(RelaTable, usize)>, Error> {
        let Some(i) = self.elf.sections.iter().position(|section| {
            use SectionKind::*;
            matches!(section.kind, RelaTable) && section.link == section_index
        }) else {
            return Ok(None);
        };
        let section = &self.elf.sections[i];
        self.file.seek(section.offset)?;
        let table = RelaTable::read(
            &mut self.file,
            self.elf.header.class,
            self.elf.header.byte_order,
            section.size,
        )?;
        Ok(Some((table, i)))
    }

    pub fn read_dynamic_table(&mut self) -> Result<DynamicTable, Error> {
        let names = self
            .elf
            .read_section_names(&mut self.file)?
            .unwrap_or_default();
        let Some(i) = self.elf.sections.iter().position(|section| {
            Some(DYNAMIC_SECTION) == names.get_string(section.name_offset as usize)
        }) else {
            return Ok(Default::default());
        };
        let section = &self.elf.sections[i];
        self.file.seek(section.offset)?;
        let table = DynamicTable::read(
            &mut self.file,
            self.elf.header.class,
            self.elf.header.byte_order,
            section.size,
        )?;
        Ok(table)
    }

    pub fn read_dynamic_string_table(&mut self) -> Result<StringTable, Error> {
        let names = self
            .elf
            .read_section_names(&mut self.file)?
            .unwrap_or_default();
        let bytes = match self.elf.sections.iter().position(|section| {
            Some(DYNSTR_SECTION) == names.get_string(section.name_offset as usize)
        }) {
            Some(i) => self.elf.sections[i].read_content(&mut self.file)?,
            None => Vec::new(),
        };
        Ok(StringTable::from(bytes))
    }

    pub fn set_dynamic_c_str(&mut self, entry_kind: DynamicTag, value: &CStr) -> Result<(), Error> {
        use DynamicTag::*;
        let mut names = self
            .elf
            .read_section_names(&mut self.file)?
            .unwrap_or_default();
        let (mut dynamic_table, old_dynamic_table_virtual_address) =
            match self.elf.sections.iter().position(|section| {
                Some(DYNAMIC_SECTION) == names.get_string(section.name_offset as usize)
            }) {
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
                    self.free_section(i, &names)?;
                    (dynamic_table, virtual_address)
                }
                None => {
                    // TODO
                    // `.dynamic` section doesn't exits. Try to find DYNAMIC segment.
                    match self
                        .elf
                        .segments
                        .iter()
                        .position(|segment| segment.kind == SegmentKind::Dynamic)
                    {
                        Some(i) => {
                            let segment = &self.elf.segments[i];
                            let virtual_address = segment.virtual_address;
                            self.file.seek(segment.offset)?;
                            let dynamic_table = DynamicTable::read(
                                &mut self.file,
                                self.elf.header.class,
                                self.elf.header.byte_order,
                                segment.file_size.min(segment.memory_size),
                            )?;
                            self.free_segment(i)?;
                            (dynamic_table, virtual_address)
                        }
                        None => (DynamicTable::default(), 0),
                    }
                }
            };
        log::trace!("Found dynamic table");
        let dynstr_table_index = match dynamic_table
            .iter()
            .find_map(|(kind, value)| (*kind == StringTableAddress).then_some(value))
        {
            Some(addr) => {
                // Find string table by its virtual address.
                self.elf.sections.iter().position(|section| {
                    section.kind == SectionKind::StringTable && section.virtual_address == *addr
                })
            }
            None => {
                // Couldn't find string table's address in the dynamic table.
                // Try to find the string table by section name.
                self.elf.sections.iter().position(|section| {
                    section.kind == SectionKind::StringTable
                        && Some(DYNSTR_SECTION) == names.get_string(section.name_offset as usize)
                })
            }
        };
        let (mut dynstr_table, dynstr_table_index) = match dynstr_table_index {
            Some(i) => {
                let bytes = self.elf.sections[i].read_content(&mut self.file)?;
                (StringTable::from(bytes), Some(i))
            }
            None => (Default::default(), None),
        };
        log::trace!("Found `.dynstr` table");
        log::trace!("dynstr table index {:?}", dynstr_table_index);
        let symbol_table_result = self.read_symbol_table()?;
        let (value_offset, dynstr_table_index) = self.get_string_offset(
            value,
            dynstr_table_index,
            DYNSTR_SECTION,
            &mut dynstr_table,
            &mut names,
        )?;
        log::trace!("dynstr table index {}", dynstr_table_index);
        // Update dynamic table.
        let dynstr_table_section = &self.elf.sections[dynstr_table_index];
        dynstr_table_section.write_out(&mut self.file, dynstr_table.as_ref())?;
        log::trace!("Updated `.dynstr` table");
        dynamic_table.set(StringTableAddress, dynstr_table_section.virtual_address);
        dynamic_table.set(StringTableSize, dynstr_table_section.size);
        dynamic_table.set(entry_kind, value_offset as u64);
        let dynamic_table_len = dynamic_table.in_file_len(self.elf.header.class) as u64;
        let name_offset = self.get_name_offset(DYNAMIC_SECTION, &mut names)?;
        let dynamic_section_index = self.alloc_section(
            Section {
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
            },
            &names,
        )?;
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
        if let Some((mut symbol_table, symbol_table_section_index)) = symbol_table_result {
            let mut changed = false;
            for symbol in symbol_table.iter_mut() {
                if symbol.address == old_dynamic_table_virtual_address {
                    log::trace!(
                        "Changed memory offset of the _DYNAMIC symbol from {:#x} to {:#x}",
                        symbol.address,
                        new_dynamic_table_virtual_address
                    );
                    symbol.address = new_dynamic_table_virtual_address;
                    changed = true;
                }
            }
            if changed {
                let section = &self.elf.sections[symbol_table_section_index];
                self.file.seek(section.offset)?;
                symbol_table.write(
                    &mut self.file,
                    self.elf.header.class,
                    self.elf.header.byte_order,
                )?;
                log::trace!("Updated symbol table");
            }
        }
        // We don't write section here since the content and the location is the same as in the
        // `.dynamic`. segment.
        //let load = Segment {
        //    kind: SegmentKind::Loadable,
        //    flags: segment.flags,
        //    virtual_address: segment.virtual_address,
        //    physical_address: segment.physical_address,
        //    offset: segment.offset,
        //    file_size: segment.file_size,
        //    memory_size: segment.memory_size,
        //    align: segment.align,
        //};
        //self.elf.segments.push(load);
        Ok(())
    }

    fn get_name_offset(&mut self, name: &CStr, names: &mut StringTable) -> Result<usize, Error> {
        let name_offset = match names.get_offset(name) {
            Some(name_offset) => {
                log::trace!("Found section name {:?} at offset {}", name, name_offset);
                name_offset
            }
            None => {
                self.free_section(self.elf.header.section_names_index as usize, names)?;
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
                        size: names.as_bytes().len() as u64,
                        link: 0,
                        info: 0,
                        align: STRING_TABLE_ALIGN,
                        entry_len: 0,
                    },
                    names,
                )?;
                self.elf.sections[i].write_out(&mut self.file, names.as_ref())?;
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
                    self.free_section(table_section_index, names)?;
                }
                let outer_string_offset = table.insert(string);
                log::trace!(
                    "Adding string {:?} to {:?} at offset {}",
                    string,
                    table_name,
                    outer_string_offset
                );
                let name_offset = self.get_name_offset(table_name, names)?;
                let i = self.alloc_section(
                    Section {
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
                    },
                    names,
                )?;
                self.elf.sections[i].write_out(&mut self.file, table.as_ref())?;
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
            self.header.class,
            self.page_size,
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

    fn free_section(&mut self, i: usize, names: &StringTable) -> Result<Section, Error> {
        let section = self.elf.sections.free(&mut self.file, i)?;
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
                .elf
                .segments
                .iter()
                .position(|segment| segment.kind == SegmentKind::Interpreter)
            {
                self.free_segment(i)?;
            }
        }
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

    fn alloc_section(&mut self, mut section: Section, names: &StringTable) -> Result<usize, Error> {
        let alloc = SpaceAllocator::new(
            self.header.class,
            self.page_size,
            &self.elf.sections,
            &mut self.elf.segments,
        );
        alloc.allocate_section(&mut section)?;
        let i = self.elf.sections.add(section);
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
        Ok(i)
    }

    fn alloc_section_header(&mut self, size: u64) -> Option<u64> {
        let alloc = SpaceAllocator::new(
            self.header.class,
            self.page_size,
            &self.elf.sections,
            &mut self.elf.segments,
        );
        alloc.allocate_file_space(size, SECTION_HEADER_ALIGN)
    }

    pub fn read_section_names(&mut self) -> Result<Option<StringTable>, Error> {
        self.elf.read_section_names(&mut self.file)
    }

    pub fn read_section(
        &mut self,
        name: &CStr,
        names: &StringTable,
    ) -> Result<Option<Vec<u8>>, Error> {
        self.elf.read_section(name, names, &mut self.file)
    }
}

impl<F> Deref for ElfPatcher<F> {
    type Target = Elf;

    fn deref(&self) -> &Self::Target {
        &self.elf
    }
}
