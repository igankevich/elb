use std::ffi::CStr;
use std::io::Read;
use std::io::Seek;
use std::io::Write;

use crate::constants::*;
use crate::Allocations;
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
        self.finish();
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

    fn finish(&mut self) {
        self.segments.finish();
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

    pub fn set_interpreter<F: Read + Write + Seek>(
        &mut self,
        mut file: F,
        interpreter: &CStr,
    ) -> Result<(), Error> {
        self.remove_interpreter(&mut file)?;
        let interpreter = interpreter.to_bytes_with_nul();
        let names = self.read_section_names(&mut file)?;
        let (name_offset, names) = self.get_name_offset(&mut file, INTERP_SECTION, names)?;
        let i = self.alloc_section(
            Section {
                name_offset,
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

    fn get_name_offset<F: Read + Write + Seek>(
        &mut self,
        mut file: F,
        name: &CStr,
        mut names: Vec<u8>,
    ) -> Result<(u32, Vec<u8>), Error> {
        let name_offset = match find_name(&names, name.to_bytes_with_nul()) {
            Some(name_offset) => {
                log::trace!("Found section name {:?} at offset {}", name, name_offset);
                name_offset
            }
            None => {
                self.free_section(&mut file, self.header.section_names_index as usize, &names)?;
                let outer_name_offset = names.len();
                log::trace!("Adding section name {:?}", name);
                names.extend_from_slice(name.to_bytes_with_nul());
                let name_offset = match find_name(&names, SHSTRTAB_SECTION.to_bytes_with_nul()) {
                    Some(name_offset) => name_offset,
                    None => {
                        let offset = names.len();
                        log::trace!("Adding section name {:?}", SHSTRTAB_SECTION);
                        names.extend_from_slice(SHSTRTAB_SECTION.to_bytes_with_nul());
                        offset
                    }
                };
                let i = self.alloc_section(
                    Section {
                        name_offset: name_offset
                            .try_into()
                            .map_err(|_| Error::TooBig("Section name"))?,
                        kind: SectionKind::ProgramData,
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
        let name_offset: u32 = name_offset
            .try_into()
            .map_err(|_| Error::TooBig("Section name"))?;
        Ok((name_offset, names))
    }

    fn free_segment<W: Write + Seek>(&mut self, writer: W, i: usize) -> Result<(), Error> {
        self.segments.free(writer, i)?;
        self.header.num_segments -= 1;
        Ok(())
    }

    #[allow(unused)]
    fn alloc_segment(&mut self, mut segment: Segment) -> Result<usize, Error> {
        segment.offset = self
            .alloc_file_block(segment.file_size)
            .ok_or(Error::FileBlockAlloc)?;
        segment.virtual_address = self
            .alloc_memory_block(segment.memory_size, segment.align, segment.offset)
            .ok_or(Error::MemoryBlockAlloc)?;
        segment.physical_address = segment.virtual_address;
        let i = self.segments.len();
        self.segments.push(segment);
        self.header.num_segments += 1;
        Ok(i)
    }

    fn free_section<W: Write + Seek>(
        &mut self,
        writer: W,
        i: usize,
        names: &[u8],
    ) -> Result<Section, Error> {
        let section = self.sections.free(writer, i)?;
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
        Ok(section)
    }

    fn alloc_section(&mut self, mut section: Section, names: &[u8]) -> Result<usize, Error> {
        section.offset = self
            .alloc_file_block(section.size)
            .ok_or(Error::FileBlockAlloc)?;
        section.virtual_address = self
            .alloc_memory_block(section.size, section.align, section.offset)
            .ok_or(Error::MemoryBlockAlloc)?;
        if self.sections.last().map(|section| section.kind) == Some(SectionKind::Null) {
            self.sections.pop();
        }
        log::trace!(
            "Adding section {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            get_name(names, section.name_offset as usize),
            section.offset,
            section.offset + section.size,
            section.virtual_address,
            section.virtual_address + section.size
        );
        let i = self.sections.len();
        self.sections.push(section);
        self.header.num_sections = self
            .sections
            .len()
            .try_into()
            .map_err(|_| Error::TooBig("No. of sections"))?;
        Ok(i)
    }

    fn alloc_file_block(&self, size: u64) -> Option<u64> {
        let mut allocations = Allocations::new();
        allocations.push(0, self.header.len as u64);
        allocations.push(
            self.header.program_header_offset,
            self.header.program_header_offset
                + self.header.segment_len as u64 * self.header.num_segments as u64,
        );
        allocations.push(
            self.header.section_header_offset,
            self.header.section_header_offset
                + self.header.section_len as u64 * self.header.num_sections as u64,
        );
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
        allocations.alloc_file_block(size)
    }

    fn alloc_memory_block(&self, size: u64, align: u64, file_offset: u64) -> Option<u64> {
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
        allocations.alloc_memory_block(size, align, file_offset)
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

fn get_name(names: &[u8], offset: usize) -> &CStr {
    let c_str_bytes = names.get(offset..).unwrap_or(&[]);
    CStr::from_bytes_until_nul(c_str_bytes).unwrap_or_default()
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
    fn test_read() {
        std::fs::copy("/tmp/wp/store/debian/usr/bin/make", "/tmp/make").unwrap();
        //std::fs::copy("/tmp/make", "/tmp/make2").unwrap();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/tmp/make")
            .unwrap();
        let mut elf = Elf::read(&mut file).unwrap();
        eprintln!("{:#?}", elf);
        /*
           let dynamic_table_entry = elf.segments.get(SegmentKind::Dynamic).unwrap();
           let mut dynamic_table = DynamicTable::read(
           &mut file,
           dynamic_table_entry,
           elf.header.class,
           elf.header.byte_order,
           )
           .unwrap();
           let string_table_address = dynamic_table
           .get(DynamicEntryKind::StringTableAddress)
           .unwrap();
           let string_table_size = dynamic_table
           .get(DynamicEntryKind::StringTableSize)
           .unwrap();
           eprintln!("String table address {:?}", string_table_address);
           eprintln!("String table size {:?}", string_table_size);
           let (string_table_index, string_table_entry) = elf
           .sections
           .iter()
           .enumerate()
           .find(|(i, entry)| {
           entry.kind == SectionKind::Strings
           && entry.virtual_address == string_table_address
           && entry.size == string_table_size
           })
           .unwrap();
           eprintln!("String table entry {:?}", string_table_entry);
           eprintln!("String table index {:?}", string_table_index);
           eprintln!("Section names index {:?}", elf.header.section_names_index);
           let dynstr_entry = elf.sections.get_mut(string_table_index).unwrap();
           dynstr_entry.align = Word::from_u64(elf.header.class, MAX_ALIGN as u64).unwrap();
           let mut dynstr_segment = {
           let dynstr_segment_index = elf
           .segments
           .iter()
           .position(|entry| {
           entry.kind == SegmentKind::Loadable
           && entry.contains_virtual_address(dynstr_entry.virtual_address)
           })
           .unwrap();
           let dynstr_segment = elf.segments.entries.remove(dynstr_segment_index);
           let (left_part, dynstr_segment, right_part) = dynstr_segment.split_off(&dynstr_entry);
           elf.segments.entries.extend(left_part);
           elf.segments.entries.extend(right_part);
           elf.header.num_segments = elf.segments.entries.len() as u16;
           dynstr_segment
           };
           let mut strings = dynstr_entry.read_content(&mut file).unwrap();
           let new_rpath = c"/tmp/wp/store/debian/lib/x86_64-linux-gnu".to_bytes_with_nul();
           let new_rpath_offset = strings.len();
           strings.extend_from_slice(new_rpath);
           dynamic_table
           .get_mut(DynamicEntryKind::StringTableSize)
           .unwrap()
           .set_usize(strings.len());
           if let Some(rpath_offset) = dynamic_table.get_mut(DynamicEntryKind::RpathOffset) {
           eprintln!("Rpath offset = {:#x}", new_rpath_offset);
           rpath_offset.set_usize(new_rpath_offset);
           } else {
           dynamic_table.push(
           DynamicEntryKind::RpathOffset,
           Word::from_u64(elf.header.class, strings.len() as u64).unwrap(),
           );
           }
           let new_virtual_address = elf
           .segments
           .iter()
           .filter(|segment| segment.kind == SegmentKind::Loadable)
           .map(|segment| segment.virtual_address.as_u64() + segment.memory_size.as_u64())
           .max()
        .unwrap_or(0)
            .next_multiple_of(MAX_ALIGN as u64);
        let new_virtual_address = Word::from_u64(elf.header.class, new_virtual_address).unwrap();
        dynstr_entry.virtual_address = new_virtual_address;
        dynstr_entry
            .write_content(&mut file, &strings, true)
            .unwrap();
        dynstr_segment.file_size = dynstr_entry.size;
        dynstr_segment.memory_size = dynstr_entry.size;
        dynstr_segment.offset = dynstr_entry.offset;
        dynstr_segment.virtual_address = new_virtual_address;
        dynstr_segment.physical_address = new_virtual_address;
        elf.segments.entries.push(dynstr_segment);
        elf.segments.entries.sort_unstable_by(|a, b| {
            if a.kind == SegmentKind::ProgramHeader {
                return Ordering::Less;
            }
            if b.kind == SegmentKind::ProgramHeader {
                return Ordering::Greater;
            }
            a.virtual_address.cmp(&b.virtual_address)
        });
        let program_header_segment_index = elf.segments.entries.iter().position(|segment| segment.kind == SegmentKind::ProgramHeader).unwrap();
        elf.segments.entries.remove(program_header_segment_index);
        //let new_program_header_len =
        //    elf.segments.entries.len() as u64 * elf.header.segment_len as u64;
        //let program_header_segment = elf.segments.get_mut(SegmentKind::ProgramHeader).unwrap();
        //program_header_segment.file_size =
        //    Word::from_u64(elf.header.class, new_program_header_len).unwrap();
        //program_header_segment.memory_size = program_header_segment.file_size;
        elf.header.num_segments = elf.segments.entries.len() as u16;
        let dynamic_table_entry = elf.segments.get_mut(SegmentKind::Dynamic).unwrap();
        dynamic_table
            .write(
                &mut file,
                dynamic_table_entry,
                elf.header.class,
                elf.header.byte_order,
            )
            .unwrap();
        */
        let new_virtual_address = elf
            .segments
            .iter()
            .filter(|segment| segment.kind == SegmentKind::Loadable)
            .map(|segment| segment.virtual_address + segment.memory_size)
            .max()
            .unwrap_or(0)
            .next_multiple_of(MAX_ALIGN as u64);
        let phdr = elf
            .segments
            .get_mut(SegmentKind::ProgramHeader)
            .unwrap()
            .move_to_end(&mut file, elf.header.class)
            .unwrap();
        phdr.virtual_address = new_virtual_address;
        let phdr_offset = phdr.offset;
        let phdr_addr = phdr.virtual_address;
        let phdr_file_size = phdr.file_size;
        let phdr_memory_size = phdr.memory_size;
        let phdr_align = phdr.align;
        elf.segments.push(Segment {
            kind: SegmentKind::Loadable,
            flags: SegmentFlags::from_bits_retain(1 << 2),
            offset: phdr_offset,
            virtual_address: phdr_addr,
            physical_address: phdr_addr,
            file_size: phdr_file_size,
            memory_size: phdr_memory_size,
            align: phdr_align,
        });
        elf.header.num_segments = elf.segments.len() as u16;
        elf.header.program_header_offset = phdr_offset;
        elf.sections.write(&mut file, &elf.header).unwrap();
        elf.segments.write(&mut file, &elf.header).unwrap();
        elf.header.write(&mut file).unwrap();
    }

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
