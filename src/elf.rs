use std::io::Read;
use std::io::Seek;

use crate::Error;
use crate::Header;
use crate::ProgramHeader;
use crate::SectionHeader;

#[derive(Debug)]
pub struct Elf {
    pub header: Header,
    pub segments: ProgramHeader,
    pub sections: SectionHeader,
}

impl Elf {
    pub fn read<R: Read + Seek>(mut reader: R) -> Result<Self, Error> {
        let header = Header::read(&mut reader)?;
        header.validate()?;
        let segments = ProgramHeader::read(&mut reader, &header)?;
        segments.validate(&header)?;
        let sections = SectionHeader::read(&mut reader, &header)?;
        sections.validate(header.class, &segments)?;
        Ok(Self {
            header,
            segments,
            sections,
        })
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.header.validate()?;
        self.segments.validate(&self.header)?;
        self.sections.validate(self.header.class, &self.segments)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::*;
    use crate::Segment;
    use crate::SegmentFlags;
    use crate::SegmentKind;
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
        if let Some(entry) = elf.segments.get_mut(SegmentKind::Interpretator) {
            let interpreter = c"/tmp/wp/store/debian/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2"
                .to_bytes_with_nul();
            entry
                .write_content(&mut file, elf.header.class, interpreter, false)
                .unwrap();
        }
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
}
