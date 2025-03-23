use std::cmp::Ordering;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ops::Range;

use crate::align_down;
use crate::align_up;
use crate::io::v2::ElfReadV2;
use crate::io::*;
use crate::other::*;
use crate::validation::*;
use crate::ByteOrder;
use crate::Class;
use crate::DynamicTag;
use crate::Error;
use crate::Header;
use crate::SegmentFlags;
use crate::SegmentKind;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct ProgramHeader {
    entries: Vec<Segment>,
}

impl ProgramHeader {
    pub fn read<R: Read + Seek>(mut reader: R, header: &Header) -> Result<Self, Error> {
        // TODO We support only u16::MAX entries. There can be more entries.
        reader.seek(SeekFrom::Start(header.program_header_offset))?;
        let mut reader = reader.take(header.segment_len as u64 * header.num_segments as u64);
        let mut entries = Vec::with_capacity(header.num_segments as usize);
        for _ in 0..header.num_segments {
            let entry = Segment::read(&mut reader, header.class, header.byte_order)?;
            entries.push(entry);
        }
        let ret = Self { entries };
        Ok(ret)
    }

    pub fn write<W: Write + Seek>(&self, mut writer: W, header: &Header) -> Result<(), Error> {
        assert_eq!(self.entries.len(), header.num_segments as usize);
        writer.seek(SeekFrom::Start(header.program_header_offset))?;
        for entry in self.entries.iter() {
            entry.write(&mut writer, header.class, header.byte_order)?;
        }
        Ok(())
    }

    pub fn read_dynamic_entries<R: Read + Seek>(
        &self,
        mut reader: R,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Vec<(DynamicTag, u64)>, Error> {
        match self
            .entries
            .iter()
            .find(|entry| entry.kind == SegmentKind::Dynamic)
        {
            Some(entry) => {
                let content = entry.read_content(&mut reader)?;
                let mut slice = &content[..];
                let word_len = class.word_len();
                let step = 2 * word_len;
                let mut entries = Vec::with_capacity(content.len() / step);
                for _ in (0..content.len()).step_by(step) {
                    let tag: DynamicTag = get_word(class, byte_order, slice).try_into()?;
                    slice = &slice[word_len..];
                    let value = get_word(class, byte_order, slice);
                    slice = &slice[word_len..];
                    entries.push((tag, value));
                }
                Ok(entries)
            }
            None => Ok(Vec::new()),
        }
    }

    pub fn validate(&self, header: &Header, page_size: u64) -> Result<(), Error> {
        for segment in self.entries.iter() {
            segment.validate(header.class)?;
        }
        self.validate_sorted()?;
        self.validate_overlap(page_size)?;
        self.validate_entry_point(header.entry_point)?;
        self.validate_phdr()?;
        Ok(())
    }

    pub fn finish(&mut self) {
        self.entries.sort_unstable_by(|a, b| {
            if a.kind == SegmentKind::ProgramHeader {
                return Ordering::Less;
            }
            if b.kind == SegmentKind::ProgramHeader {
                return Ordering::Greater;
            }
            a.virtual_address.cmp(&b.virtual_address)
        });
    }

    fn validate_sorted(&self) -> Result<(), Error> {
        let mut prev: Option<&Segment> = None;
        for segment in self.entries.iter() {
            if segment.kind != SegmentKind::Loadable {
                continue;
            }
            if let Some(prev) = prev.as_ref() {
                let segment_start = segment.virtual_address;
                let prev_start = prev.virtual_address;
                if prev_start > segment_start {
                    return Err(Error::SegmentsNotSorted);
                }
            }
            prev = Some(segment);
        }
        Ok(())
    }

    fn validate_overlap(&self, page_size: u64) -> Result<(), Error> {
        let filters = [
            |segment: &Segment, page_size: u64| {
                if segment.kind != SegmentKind::Loadable {
                    return None;
                }
                // GNU libc ld.so expands virtual address space of each segment
                // to the nearest page boundaries.
                let segment_start = align_down(segment.virtual_address, page_size);
                let segment_end = align_up(segment_start + segment.memory_size, page_size);
                if segment_start == segment_end {
                    return None;
                }
                Some(segment_start..segment_end)
            },
            |segment: &Segment, _page_size: u64| {
                if segment.kind != SegmentKind::Loadable {
                    return None;
                }
                let segment_start = segment.offset;
                let segment_end = segment_start + segment.file_size;
                if segment_start == segment_end {
                    return None;
                }
                Some(segment_start..segment_end)
            },
        ];
        for filter in filters.into_iter() {
            let mut ranges = self
                .entries
                .iter()
                .filter_map(|segment| filter(segment, page_size))
                .collect::<Vec<_>>();
            ranges.sort_unstable_by_key(|segment| segment.start);
            for i in 1..ranges.len() {
                let cur = &ranges[i];
                let prev = &ranges[i - 1];
                if prev.end > cur.start {
                    return Err(Error::SegmentsOverlap(
                        prev.start, prev.end, cur.start, cur.end,
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_entry_point(&self, entry_point: u64) -> Result<(), Error> {
        if entry_point != 0
            && !self.entries.iter().any(|segment| {
                segment.kind == SegmentKind::Loadable
                    && segment.virtual_address_range().contains(&entry_point)
            })
        {
            return Err(Error::InvalidEntryPoint(entry_point));
        }
        Ok(())
    }

    fn validate_phdr(&self) -> Result<(), Error> {
        let mut phdr = None;
        let mut load_found = false;
        for segment in self.entries.iter() {
            match segment.kind {
                SegmentKind::ProgramHeader => {
                    if load_found {
                        return Err(Error::InvalidProgramHeaderSegment(
                            "PHDR segment should come before any LOAD segment",
                        ));
                    }
                    phdr = Some(segment);
                }
                SegmentKind::Loadable => {
                    load_found = true;
                }
                _ => {}
            }
            if load_found && phdr.is_some() {
                break;
            }
        }
        if let Some(phdr) = phdr {
            if !self.entries.iter().any(|segment| {
                if segment.kind != SegmentKind::Loadable {
                    return false;
                }
                let segment_start = segment.virtual_address;
                let segment_end = segment_start + segment.memory_size;
                let phdr_start = phdr.virtual_address;
                let phdr_end = phdr_start + phdr.memory_size;
                segment_start <= phdr_start && phdr_start <= segment_end && phdr_end <= segment_end
            }) {
                return Err(Error::InvalidProgramHeaderSegment(
                    "PHDR segment should be covered by a LOAD segment",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn free<W: Write + Seek>(&mut self, writer: W, i: usize) -> Result<Segment, Error> {
        let segment = self.entries.remove(i);
        segment.clear_content(writer)?;
        Ok(segment)
    }

    pub(crate) fn add(&mut self, segment: Segment) -> usize {
        // Append null segments.
        if segment.kind == SegmentKind::Null {
            let i = self.entries.len();
            self.entries.push(segment);
            return i;
        }
        let spare_index = self
            .entries
            .iter()
            .position(|segment| segment.kind == SegmentKind::Null);
        let i = match spare_index {
            Some(i) => {
                // Replace null segment with the new one.
                self.entries[i] = segment;
                i
            }
            None => {
                // No null segments found. Append the new one.
                let i = self.entries.len();
                self.entries.push(segment);
                i
            }
        };
        let segment = &self.entries[i];
        log::trace!(
            "Adding segment [{i}] {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            segment.kind,
            segment.offset,
            segment.offset + segment.file_size,
            segment.virtual_address,
            segment.virtual_address + segment.memory_size
        );
        i
    }

    /// Is the file offsets range covered by LOAD segment?
    pub(crate) fn is_loadable(&self, file_offsets: Range<u64>) -> bool {
        self.entries.iter().any(|segment| {
            segment.kind == SegmentKind::Loadable
                && segment.offset <= file_offsets.start
                && file_offsets.end <= segment.offset + segment.file_size
        })
    }
}

impl Deref for ProgramHeader {
    type Target = Vec<Segment>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl DerefMut for ProgramHeader {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Segment {
    pub kind: SegmentKind,
    pub flags: SegmentFlags,
    pub offset: u64,
    pub virtual_address: u64,
    pub physical_address: u64,
    pub file_size: u64,
    pub memory_size: u64,
    pub align: u64,
}

impl Segment {
    pub fn read<R: ElfReadV2>(
        mut reader: R,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Self, Error> {
        let kind: SegmentKind = reader.read_u32(byte_order)?.try_into()?;
        let mut flags = 0;
        if class == Class::Elf64 {
            flags = reader.read_u32(byte_order)?;
        }
        let offset = reader.read_word(class, byte_order)?;
        let virtual_address = reader.read_word(class, byte_order)?;
        let physical_address = reader.read_word(class, byte_order)?;
        let file_size = reader.read_word(class, byte_order)?;
        let memory_size = reader.read_word(class, byte_order)?;
        if class == Class::Elf32 {
            flags = reader.read_u32(byte_order)?;
        }
        let align = reader.read_word(class, byte_order)?;
        Ok(Self {
            kind,
            flags: SegmentFlags::from_bits_retain(flags),
            offset,
            virtual_address,
            physical_address,
            file_size,
            memory_size,
            align,
        })
    }

    pub fn write<W: ElfWrite>(
        &self,
        mut writer: W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        writer.write_u32(byte_order, self.kind.as_u32())?;
        if class == Class::Elf64 {
            writer.write_u32(byte_order, self.flags.bits())?;
        }
        writer.write_word(class, byte_order, self.offset)?;
        writer.write_word(class, byte_order, self.virtual_address)?;
        writer.write_word(class, byte_order, self.physical_address)?;
        writer.write_word(class, byte_order, self.file_size)?;
        writer.write_word(class, byte_order, self.memory_size)?;
        if class == Class::Elf32 {
            writer.write_u32(byte_order, self.flags.bits())?;
        }
        writer.write_word(class, byte_order, self.align)?;
        Ok(())
    }

    pub fn read_content<R: Read + Seek>(&self, mut reader: R) -> Result<Vec<u8>, Error> {
        reader.seek(SeekFrom::Start(self.offset))?;
        let n: usize = self
            .file_size
            .try_into()
            .map_err(|_| Error::TooBig("in-file-size"))?;
        let mut buf = vec![0_u8; n];
        reader.read_exact(&mut buf[..])?;
        Ok(buf)
    }

    pub fn write_out<W: Write + Seek>(&self, mut writer: W, content: &[u8]) -> Result<(), Error> {
        assert_eq!(self.file_size, content.len() as u64);
        writer.seek(SeekFrom::Start(self.offset))?;
        writer.write_all(content)?;
        Ok(())
    }

    /// Zero out the entry's content.
    pub fn clear_content<W: Write + Seek>(&self, writer: W) -> Result<(), Error> {
        zero(writer, self.offset, self.file_size)?;
        Ok(())
    }

    /// Physical address range.
    pub const fn physical_address_range(&self) -> Range<u64> {
        let start = self.physical_address;
        let end = start + self.memory_size;
        start..end
    }

    /// Virtual address range.
    pub const fn virtual_address_range(&self) -> Range<u64> {
        let start = self.virtual_address;
        let end = start + self.memory_size;
        start..end
    }

    /// In-file location of the segment.
    pub const fn file_offset_range(&self) -> Range<u64> {
        let start = self.offset;
        let end = start + self.file_size;
        start..end
    }

    pub fn validate(&self, class: Class) -> Result<(), Error> {
        self.validate_overflow(class)?;
        self.validate_align()?;
        Ok(())
    }

    fn validate_overflow(&self, class: Class) -> Result<(), Error> {
        match class {
            Class::Elf32 => {
                validate_u32(self.offset, "Segment offset")?;
                validate_u32(self.virtual_address, "Segment virtual address")?;
                validate_u32(self.physical_address, "Segment physical address")?;
                validate_u32(self.file_size, "Segment in-file size")?;
                validate_u32(self.memory_size, "Segment in-memory size")?;
                validate_u32(self.align, "Segment align")?;
                let offset = self.offset as u32;
                let file_size = self.file_size as u32;
                let virtual_address = self.virtual_address as u32;
                let physical_address = self.physical_address as u32;
                let memory_size = self.memory_size as u32;
                if offset.checked_add(file_size).is_none() {
                    return Err(Error::TooBig("Segment in-file size"));
                }
                if virtual_address.checked_add(memory_size).is_none()
                    || physical_address.checked_add(memory_size).is_none()
                {
                    return Err(Error::TooBig("Segment in-memory size"));
                }
            }
            Class::Elf64 => {
                if self.offset.checked_add(self.file_size).is_none() {
                    return Err(Error::TooBig("Segment in-file size"));
                }
                if self.virtual_address.checked_add(self.memory_size).is_none()
                    || self
                        .physical_address
                        .checked_add(self.memory_size)
                        .is_none()
                {
                    return Err(Error::TooBig("Segment in-memory size"));
                }
            }
        }
        Ok(())
    }

    fn validate_align(&self) -> Result<(), Error> {
        if !align_is_valid(self.align) {
            return Err(Error::InvalidAlign(self.align));
        }
        if self.kind == SegmentKind::Loadable
            && self.align != 0
            && self.offset % self.align != self.virtual_address % self.align
        {
            let file_start = self.virtual_address;
            let file_end = file_start + self.file_size;
            let memory_start = self.virtual_address;
            let memory_end = memory_start + self.memory_size;
            return Err(Error::MisalignedSegment(
                file_start,
                file_end,
                memory_start,
                memory_end,
                self.align,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    use arbitrary::Unstructured;
    use arbtest::arbtest;

    use crate::constants::*;
    use crate::FileKind;

    #[test]
    fn section_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let expected = Segment::arbitrary(u, class)?;
            let mut buf = Vec::new();
            expected.write(&mut buf, class, byte_order).unwrap();
            let actual = Segment::read(&buf[..], class, byte_order).unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    #[test]
    fn program_header_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let entry_len = class.segment_len();
            let expected = ProgramHeader::arbitrary(u, class)?;
            let mut cursor = Cursor::new(Vec::new());
            let header = Header {
                num_segments: expected.len().try_into().unwrap(),
                segment_len: entry_len,
                program_header_offset: 0,
                class,
                byte_order,
                os_abi: 0.into(),
                abi_version: 0,
                kind: FileKind::Executable,
                machine: 0.into(),
                flags: 0,
                entry_point: 0,
                section_header_offset: class.arbitrary_word(u)?,
                section_len: 0,
                num_sections: 0,
                section_names_index: 0,
                len: class.header_len(),
            };
            expected
                .write(&mut cursor, &header)
                .inspect_err(|e| panic!("Failed to write {:#?} {:#?}: {e}", header, expected))
                .unwrap();
            cursor.set_position(0);
            let actual = ProgramHeader::read(&mut cursor, &header)
                .inspect_err(|e| panic!("Failed to read {:#?} {:#?}: {e}", header, expected))
                .unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    impl ProgramHeader {
        pub fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let num_entries = u.arbitrary_len::<[u8; SEGMENT_LEN_64]>()?;
            let mut entries: Vec<Segment> = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(Segment::arbitrary(u, class)?);
            }
            Ok(ProgramHeader { entries })
        }
    }

    impl Segment {
        pub fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            Ok(Self {
                kind: u.arbitrary()?,
                flags: SegmentFlags::from_bits_retain(u.arbitrary()?),
                offset: class.arbitrary_word(u)?,
                virtual_address: class.arbitrary_word(u)?,
                physical_address: class.arbitrary_word(u)?,
                file_size: class.arbitrary_word(u)?,
                memory_size: class.arbitrary_word(u)?,
                align: class.arbitrary_align(u)?,
            })
        }
    }
}
