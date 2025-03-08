use std::collections::BTreeSet;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ops::Range;

use crate::constants::*;
use crate::ByteOrder;
use crate::Class;
use crate::DynamicEntryKind;
use crate::Error;
use crate::FileKind;
use crate::SectionFlags;
use crate::SectionKind;
use crate::SegmentFlags;
use crate::SegmentKind;
use crate::Word;

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
        sections.validate(&segments)?;
        Ok(Self {
            header,
            segments,
            sections,
        })
    }

    pub fn allocations(&self) -> BTreeSet<(u64, u64)> {
        let mut ranges = BTreeSet::new();
        ranges.insert((0_u64, self.header.len as u64));
        ranges.insert((
            self.header.program_header_offset.as_u64(),
            self.header.program_header_offset.as_u64()
                + self.header.segment_len as u64 * self.header.num_segments as u64,
        ));
        ranges.insert((
            self.header.section_header_offset.as_u64(),
            self.header.section_header_offset.as_u64()
                + self.header.section_len as u64 * self.header.num_sections as u64,
        ));
        for entry in self.segments.entries.iter() {
            ranges.insert((
                entry.offset.as_u64(),
                entry.offset.as_u64() + entry.file_size.as_u64(),
            ));
        }
        for entry in self.sections.entries.iter() {
            ranges.insert((
                entry.offset.as_u64(),
                entry.offset.as_u64() + entry.size.as_u64(),
            ));
        }
        ranges
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.header.validate()?;
        self.segments.validate(&self.header)?;
        self.sections.validate(&self.segments)?;
        Ok(())
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Header {
    pub class: Class,
    pub byte_order: ByteOrder,
    pub os_abi: u8,
    pub abi_version: u8,
    pub kind: FileKind,
    pub machine: u16,
    pub flags: u32,
    pub entry_point: Word,
    pub program_header_offset: Word,
    pub segment_len: u16,
    pub num_segments: u16,
    pub section_header_offset: Word,
    pub section_len: u16,
    pub num_sections: u16,
    pub section_names_index: u16,
    pub len: u16,
}

impl Header {
    pub fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let mut buf = [0_u8; HEADER_LEN_64];
        reader.read_exact(&mut buf[..5])?;
        if buf[..MAGIC.len()] != MAGIC {
            return Err(Error::NotElf);
        }
        let class: Class = buf[4].try_into()?;
        let header_len = match class {
            Class::Elf32 => HEADER_LEN_32,
            Class::Elf64 => HEADER_LEN_64,
        };
        reader.read_exact(&mut buf[5..header_len])?;
        let byte_order: ByteOrder = buf[5].try_into()?;
        let version = buf[6];
        if version != VERSION {
            return Err(Error::InvalidVersion(version));
        }
        let os_abi = buf[7];
        let abi_version = buf[8];
        let kind: FileKind = byte_order.get_u16(&buf[16..18]).try_into()?;
        let machine = byte_order.get_u16(&buf[18..20]);
        let version = buf[20];
        if version != VERSION {
            return Err(Error::InvalidVersion(version));
        }
        let word_len = class.word_len();
        let entry_point = Word::new(class, byte_order, &buf[24..]);
        let slice = &buf[24 + word_len..];
        let program_header_offset = Word::new(class, byte_order, slice);
        let slice = &slice[word_len..];
        let section_header_offset = Word::new(class, byte_order, slice);
        let slice = &slice[word_len..];
        let flags = byte_order.get_u32(slice);
        let slice = &slice[4..];
        let real_header_len = byte_order.get_u16(slice);
        let slice = &slice[2..];
        let segment_len = byte_order.get_u16(slice);
        let slice = &slice[2..];
        let num_segments = byte_order.get_u16(slice);
        let slice = &slice[2..];
        let section_len = byte_order.get_u16(slice);
        let slice = &slice[2..];
        let num_sections = byte_order.get_u16(slice);
        let slice = &slice[2..];
        let section_names_index = byte_order.get_u16(slice);
        if real_header_len as usize > header_len {
            // Throw away padding bytes.
            std::io::copy(
                &mut reader.take(real_header_len as u64 - header_len as u64),
                &mut std::io::empty(),
            )?;
        }
        let ret = Self {
            class,
            byte_order,
            os_abi,
            abi_version,
            kind,
            machine,
            flags,
            entry_point,
            program_header_offset,
            segment_len,
            num_segments,
            section_header_offset,
            section_len,
            num_sections,
            section_names_index,
            len: real_header_len,
        };
        Ok(ret)
    }

    pub fn write<W: Write + Seek>(&self, mut writer: W) -> Result<(), Error> {
        self.validate()?;
        let mut buf = [0_u8; HEADER_LEN_64];
        buf[..MAGIC.len()].copy_from_slice(&MAGIC);
        buf[4] = self.class as u8;
        buf[5] = self.byte_order as u8;
        buf[6] = VERSION;
        buf[7] = self.os_abi;
        buf[8] = self.abi_version;
        self.byte_order
            .write_u16(&mut buf[16..], self.kind.as_u16())?;
        self.byte_order.write_u16(&mut buf[18..], self.machine)?;
        buf[20] = VERSION;
        let word_len = self.class.word_len();
        let mut offset = 24;
        self.entry_point
            .write(&mut buf[offset..], self.byte_order)?;
        offset += word_len;
        self.program_header_offset
            .write(&mut buf[offset..], self.byte_order)?;
        offset += word_len;
        self.section_header_offset
            .write(&mut buf[offset..], self.byte_order)?;
        offset += word_len;
        self.byte_order.write_u32(&mut buf[offset..], self.flags)?;
        offset += 4;
        self.byte_order.write_u16(&mut buf[offset..], self.len)?;
        offset += 2;
        self.byte_order
            .write_u16(&mut buf[offset..], self.segment_len)?;
        offset += 2;
        self.byte_order
            .write_u16(&mut buf[offset..], self.num_segments)?;
        offset += 2;
        self.byte_order
            .write_u16(&mut buf[offset..], self.section_len)?;
        offset += 2;
        self.byte_order
            .write_u16(&mut buf[offset..], self.num_sections)?;
        offset += 2;
        self.byte_order
            .write_u16(&mut buf[offset..], self.section_names_index)?;
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&buf[..self.len as usize])?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.len > HEADER_LEN_64 as u16 {
            return Err(Error::InvalidHeaderLen(self.len));
        }
        if self.section_len != self.class.section_len() {
            return Err(Error::InvalidSectionLen(self.section_len));
        }
        if self.segment_len != self.class.segment_len() {
            return Err(Error::InvalidSegmentLen(self.segment_len));
        }
        let segments_start = self.program_header_offset.as_u64();
        let segments_end = (self.segment_len as u64)
            .checked_mul(self.num_segments.into())
            .ok_or(Error::TooBig("No. of segments is too big"))?
            .checked_add(segments_start)
            .ok_or(Error::TooBig("No. of segments is too big"))?;
        let sections_start = self.section_header_offset.as_u64();
        let sections_end = (self.segment_len as u64)
            .checked_mul(self.num_sections.into())
            .ok_or(Error::TooBig("No. of sections is too big"))?
            .checked_add(sections_start)
            .ok_or(Error::TooBig("No. of sections is too big"))?;
        let segments_range = segments_start..segments_end;
        let sections_range = sections_start..sections_end;
        if blocks_overlap(&segments_range, &sections_range) {
            return Err(Error::Overlap("Segments and sections overlap"));
        }
        if self.section_names_index != 0 && self.section_names_index > self.num_sections {
            return Err(Error::InvalidSectionHeaderStringTableIndex(
                self.section_names_index,
            ));
        }
        Ok(())
    }
}

/// Check that memory/file blocks don't overlap.
const fn blocks_overlap(a: &Range<u64>, b: &Range<u64>) -> bool {
    if a.start == a.end || b.start == b.end {
        return false;
    }
    if a.end == b.start || b.end == a.start {
        return false;
    }
    a.start < b.end && b.start < a.end
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct ProgramHeader {
    entries: Vec<Segment>,
}

impl ProgramHeader {
    pub fn read<R: Read + Seek>(mut reader: R, header: &Header) -> Result<Self, Error> {
        // TODO We support only u16::MAX entries. There can be more entries.
        reader.seek(SeekFrom::Start(header.program_header_offset.as_u64()))?;
        let mut reader = reader.take(header.segment_len as u64 * header.num_segments as u64);
        let mut entries = Vec::with_capacity(header.num_segments as usize);
        for _ in 0..header.num_segments {
            let entry = Segment::read(
                &mut reader,
                header.class,
                header.byte_order,
                header.segment_len,
            )?;
            entries.push(entry);
        }
        let ret = Self { entries };
        Ok(ret)
    }

    pub fn write<W: Write + Seek>(&self, mut writer: W, header: &Header) -> Result<(), Error> {
        assert_eq!(self.entries.len(), header.num_segments as usize);
        writer.seek(SeekFrom::Start(header.program_header_offset.as_u64()))?;
        for entry in self.entries.iter() {
            entry.write(
                &mut writer,
                header.class,
                header.byte_order,
                header.segment_len,
            )?;
        }
        Ok(())
    }

    pub fn get(&self, kind: SegmentKind) -> Option<&Segment> {
        self.entries.iter().find(|entry| entry.kind == kind)
    }

    pub fn get_mut(&mut self, kind: SegmentKind) -> Option<&mut Segment> {
        self.entries.iter_mut().find(|entry| entry.kind == kind)
    }

    pub fn read_dynamic_entries<R: Read + Seek>(
        &self,
        mut reader: R,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Vec<(DynamicEntryKind, Word)>, Error> {
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
                    let tag: DynamicEntryKind = Word::new(class, byte_order, slice).try_into()?;
                    slice = &slice[word_len..];
                    let value = Word::new(class, byte_order, slice);
                    slice = &slice[word_len..];
                    entries.push((tag, value));
                }
                Ok(entries)
            }
            None => Ok(Vec::new()),
        }
    }

    pub fn validate(&self, header: &Header) -> Result<(), Error> {
        for segment in self.entries.iter() {
            segment.validate()?;
        }
        self.validate_sorted()?;
        self.validate_overlap()?;
        self.validate_entry_point(header.entry_point.as_u64())?;
        self.validate_phdr()?;
        Ok(())
    }

    fn validate_sorted(&self) -> Result<(), Error> {
        let mut prev: Option<&Segment> = None;
        for segment in self.entries.iter() {
            if segment.kind != SegmentKind::Loadable {
                continue;
            }
            if let Some(prev) = prev.as_ref() {
                let segment_start = segment.virtual_address.as_u64();
                let prev_start = prev.virtual_address.as_u64();
                if prev_start > segment_start {
                    return Err(Error::SegmentsNotSorted);
                }
            }
            prev = Some(segment);
        }
        Ok(())
    }

    fn validate_overlap(&self) -> Result<(), Error> {
        let filters = [
            |segment: &Segment| {
                if segment.kind != SegmentKind::Loadable {
                    return None;
                }
                let segment_start = segment.virtual_address.as_u64();
                let segment_end = segment_start + segment.memory_size.as_u64();
                if segment_start == segment_end {
                    return None;
                }
                Some(segment_start..segment_end)
            },
            |segment: &Segment| {
                if segment.kind != SegmentKind::Loadable {
                    return None;
                }
                let segment_start = segment.offset.as_u64();
                let segment_end = segment_start + segment.file_size.as_u64();
                if segment_start == segment_end {
                    return None;
                }
                Some(segment_start..segment_end)
            },
        ];
        for filter in filters.into_iter() {
            let mut ranges = self.entries.iter().filter_map(filter).collect::<Vec<_>>();
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
                    && segment.contains_virtual_address(entry_point)
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
                    if phdr.is_none() {
                        return Err(Error::InvalidProgramHeaderSegment(
                            "PHDR segment should come before any LOAD segment",
                        ));
                    }
                    load_found = true;
                }
                _ => {}
            }
            if load_found && phdr.is_some() {
                break;
            }
        }
        let Some(phdr) = phdr else {
            return Err(Error::InvalidProgramHeaderSegment("No PHDR segment"));
        };
        if !self.entries.iter().any(|segment| {
            if segment.kind != SegmentKind::Loadable {
                return false;
            }
            let segment_start = segment.virtual_address.as_u64();
            let segment_end = segment_start + segment.memory_size.as_u64();
            let phdr_start = phdr.virtual_address.as_u64();
            let phdr_end = phdr_start + phdr.memory_size.as_u64();
            segment_start <= phdr_start && phdr_start <= segment_end && phdr_end <= segment_end
        }) {
            return Err(Error::InvalidProgramHeaderSegment(
                "PHDR segment should be covered by a LOAD segment",
            ));
        }
        Ok(())
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
    pub offset: Word,
    pub virtual_address: Word,
    pub physical_address: Word,
    pub file_size: Word,
    pub memory_size: Word,
    pub align: Word,
}

impl Segment {
    pub fn read<R: Read>(
        mut reader: R,
        class: Class,
        byte_order: ByteOrder,
        entry_len: u16,
    ) -> Result<Self, Error> {
        assert_eq!(class.segment_len(), entry_len);
        let mut buf = [0_u8; MAX_SEGMENT_LEN];
        reader.read_exact(&mut buf[..entry_len as usize])?;
        let slice = &buf[..];
        let kind: SegmentKind = byte_order.get_u32(slice).try_into()?;
        let (flags_offset, slice) = match class {
            Class::Elf32 => (24, &slice[4..]),
            Class::Elf64 => (4, &slice[8..]),
        };
        let flags = byte_order.get_u32(&buf[flags_offset..]);
        let offset = Word::new(class, byte_order, slice);
        let slice = &slice[offset.size()..];
        let virtual_address = Word::new(class, byte_order, slice);
        let slice = &slice[virtual_address.size()..];
        let physical_address = Word::new(class, byte_order, slice);
        let slice = &slice[physical_address.size()..];
        let file_size = Word::new(class, byte_order, slice);
        let slice = &slice[file_size.size()..];
        let memory_size = Word::new(class, byte_order, slice);
        let slice = &slice[memory_size.size()..];
        let align_offset = match class {
            Class::Elf32 => 4,
            Class::Elf64 => 0,
        };
        let align = Word::new(class, byte_order, &slice[align_offset..]);
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

    pub fn write<W: Write>(
        &self,
        mut writer: W,
        class: Class,
        byte_order: ByteOrder,
        entry_len: u16,
    ) -> Result<(), Error> {
        assert_eq!(class.segment_len(), entry_len);
        let mut buf = Vec::with_capacity(entry_len as usize);
        byte_order.write_u32(&mut buf, self.kind.as_u32())?;
        if class == Class::Elf64 {
            byte_order.write_u32(&mut buf, self.flags.bits())?;
        }
        self.offset.write(&mut buf, byte_order)?;
        self.virtual_address.write(&mut buf, byte_order)?;
        self.physical_address.write(&mut buf, byte_order)?;
        self.file_size.write(&mut buf, byte_order)?;
        self.memory_size.write(&mut buf, byte_order)?;
        if class == Class::Elf32 {
            byte_order.write_u32(&mut buf, self.flags.bits())?;
        }
        self.align.write(&mut buf, byte_order)?;
        writer.write_all(&buf)?;
        Ok(())
    }

    pub fn read_content<R: Read + Seek>(&self, mut reader: R) -> Result<Vec<u8>, Error> {
        reader.seek(SeekFrom::Start(self.offset.as_u64()))?;
        let mut buf = vec![0_u8; self.file_size.as_usize()];
        reader.read_exact(&mut buf[..])?;
        Ok(buf)
    }

    pub fn write_content<W: Write + Seek>(
        &mut self,
        writer: W,
        content: &[u8],
        no_overwrite: bool,
    ) -> Result<(), Error> {
        let (offset, file_size) = store(
            writer,
            self.offset,
            self.file_size,
            self.align.as_u64().max(MAX_ALIGN as u64),
            content,
            no_overwrite,
        )?;
        self.offset = offset;
        let old_file_size = self.file_size.as_u64();
        let new_file_size = file_size.as_u64();
        let old_memory_size = self.memory_size.as_u64();
        let new_memory_size = if old_file_size > new_file_size {
            old_memory_size - (old_file_size - new_file_size)
        } else {
            old_memory_size + (old_file_size - new_file_size)
        };
        eprintln!(
            "Old size -> new size: {:?} -> {:?}",
            self.memory_size, new_memory_size
        );
        eprintln!(
            "Old file size -> new file size: {:?} -> {:?}",
            self.file_size, file_size
        );
        self.memory_size.set_u64(new_memory_size)?;
        self.file_size = file_size;
        Ok(())
    }

    pub fn move_to_end<F: Read + Write + Seek>(&mut self, mut file: F) -> Result<&mut Self, Error> {
        let content = self.read_content(&mut file)?;
        let no_overwrite = true;
        self.write_content(&mut file, &content, no_overwrite)?;
        Ok(self)
    }

    pub fn contains_virtual_address(&self, addr: u64) -> bool {
        let start = self.virtual_address.as_u64();
        let end = start + self.memory_size.as_u64();
        (start..end).contains(&addr)
    }

    pub fn contains_file_offset(&self, offset: u64) -> bool {
        let start = self.offset.as_u64();
        let end = start + self.file_size.as_u64();
        (start..end).contains(&offset)
    }

    #[must_use]
    pub fn split_off(self, section: &Section) -> (Option<Self>, Self, Option<Self>) {
        let section_address_start = section.virtual_address.as_u64();
        let section_address_end = section_address_start + section.size.as_u64();
        let segment_start = self.virtual_address.as_u64();
        let segment_end = segment_start + self.memory_size.as_u64();
        let segment_before_range = segment_start..section_address_start;
        let segment_after_range = section_address_end..segment_end;
        // Left segment.
        let class = self.file_size.class();
        let left = if !segment_before_range.is_empty() {
            let file_size =
                Word::from_u64(class, segment_before_range.end - segment_before_range.start)
                    .expect("A smaller value than the current one should fit into a word");
            Some(Self {
                kind: self.kind,
                flags: self.flags,
                offset: self.offset,
                virtual_address: self.virtual_address,
                physical_address: self.physical_address,
                file_size,
                memory_size: file_size,
                align: self.align,
            })
        } else {
            None
        };
        // Middle segment.
        let middle = Self {
            kind: self.kind,
            flags: self.flags,
            offset: section.offset,
            virtual_address: section.virtual_address,
            physical_address: section.virtual_address,
            file_size: section.size,
            memory_size: section.size,
            align: section.align,
        };
        // Right segment.
        let right = if !segment_after_range.is_empty() {
            // TODO
            let mut segment_after_range = segment_after_range;
            segment_after_range.start = segment_start;
            let file_size =
                Word::from_u64(class, segment_after_range.end - segment_after_range.start)
                    .expect("A smaller value than the current one should fit into a word");
            let virtual_address = Word::from_u64(class, segment_after_range.start)
                .expect("We checked for overflow in `validate`");
            let offset = Word::from_u64(
                class,
                self.offset.as_u64() + (segment_after_range.start - segment_start),
            )
            .expect("Should not overflow");
            // Let's guess the alignment since we don't know which sections are in this segment's part.
            let align = {
                let a = virtual_address.as_u64();
                let o = offset.as_u64();
                let mut align = MAX_ALIGN as u64;
                let align = loop {
                    if align <= 1 {
                        break 1;
                    }
                    if align <= a && align <= o && a % align == 0 && o % align == 0 {
                        break align;
                    }
                    align >>= 1;
                };
                Word::from_u64(class, align).expect("Never overflows")
            };
            Some(Self {
                kind: self.kind,
                flags: self.flags,
                offset,
                virtual_address,
                physical_address: virtual_address,
                file_size,
                memory_size: file_size,
                align,
            })
        } else {
            None
        };
        (left, middle, right)
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.validate_overflow()?;
        self.validate_align()?;
        Ok(())
    }

    fn validate_overflow(&self) -> Result<(), Error> {
        if self
            .offset
            .as_u64()
            .checked_add(self.file_size.as_u64())
            .is_none()
        {
            return Err(Error::TooBig("Segment in-file size is too big"));
        }
        if self
            .virtual_address
            .as_u64()
            .checked_add(self.memory_size.as_u64())
            .is_none()
        {
            return Err(Error::TooBig("Segment in-memory size is too big"));
        }
        Ok(())
    }

    fn validate_align(&self) -> Result<(), Error> {
        let align = self.align.as_u64();
        if !align_is_valid(align) {
            return Err(Error::InvalidAlign(align));
        }
        if self.kind == SegmentKind::Loadable
            && align != 0
            && self.offset.as_u64() % align != self.virtual_address.as_u64() % align
        {
            let file_start = self.virtual_address.as_u64();
            let file_end = file_start + self.file_size.as_u64();
            let memory_start = self.virtual_address.as_u64();
            let memory_end = memory_start + self.memory_size.as_u64();
            return Err(Error::MisalignedSegment(
                file_start,
                file_end,
                memory_start,
                memory_end,
                align,
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct DynamicTable {
    entries: Vec<(DynamicEntryKind, Word)>,
}

impl DynamicTable {
    pub fn read<R: Read + Seek>(
        reader: R,
        entry: &Segment,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Self, Error> {
        let content = entry.read_content(reader)?;
        let mut slice = &content[..];
        let word_len = class.word_len();
        let step = 2 * word_len;
        let mut entries = Vec::with_capacity(content.len() / step);
        for _ in (0..content.len()).step_by(step) {
            let tag: DynamicEntryKind = Word::new(class, byte_order, slice).try_into()?;
            slice = &slice[word_len..];
            let value = Word::new(class, byte_order, slice);
            slice = &slice[word_len..];
            entries.push((tag, value));
        }
        Ok(Self { entries })
    }

    pub fn write<W: Write + Seek>(
        &self,
        writer: W,
        entry: &mut Segment,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        let mut content = Vec::new();
        for (kind, value) in self.entries.iter() {
            kind.to_word(class).write(&mut content, byte_order)?;
            value.write(&mut content, byte_order)?;
        }
        entry.write_content(writer, &content, false)?;
        Ok(())
    }

    pub fn get(&self, kind: DynamicEntryKind) -> Option<Word> {
        self.entries
            .iter()
            .find_map(|(k, value)| (*k == kind).then_some(*value))
    }

    pub fn get_mut(&mut self, kind: DynamicEntryKind) -> Option<&mut Word> {
        self.entries
            .iter_mut()
            .find_map(|(k, value)| (*k == kind).then_some(value))
    }

    pub fn push(&mut self, kind: DynamicEntryKind, value: Word) {
        self.entries.push((kind, value));
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct SectionHeader {
    entries: Vec<Section>,
}

impl SectionHeader {
    pub fn read<R: Read + Seek>(mut reader: R, header: &Header) -> Result<Self, Error> {
        reader.seek(SeekFrom::Start(header.section_header_offset.as_u64()))?;
        let mut reader = reader.take(header.section_len as u64 * header.num_sections as u64);
        let mut entries = Vec::with_capacity(header.num_sections as usize);
        for _ in 0..header.num_sections {
            let entry = Section::read(
                &mut reader,
                header.class,
                header.byte_order,
                header.section_len,
            )?;
            entries.push(entry);
        }
        let ret = Self { entries };
        Ok(ret)
    }

    pub fn write<W: Write + Seek>(&self, mut writer: W, header: &Header) -> Result<(), Error> {
        assert_eq!(self.entries.len(), header.num_sections as usize);
        writer.seek(SeekFrom::Start(header.section_header_offset.as_u64()))?;
        for entry in self.entries.iter() {
            entry.write(
                &mut writer,
                header.class,
                header.byte_order,
                header.section_len,
            )?;
        }
        Ok(())
    }

    pub fn validate(&self, program_header: &ProgramHeader) -> Result<(), Error> {
        for section in self.entries.iter() {
            section.validate(program_header)?;
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

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Section {
    pub name: u32,
    pub kind: SectionKind,
    pub flags: SectionFlags,
    pub virtual_address: Word,
    pub offset: Word,
    pub size: Word,
    pub link: u32,
    pub info: u32,
    pub align: Word,
    pub entry_len: Word,
}

impl Section {
    pub fn read<R: Read>(
        mut reader: R,
        class: Class,
        byte_order: ByteOrder,
        entry_len: u16,
    ) -> Result<Self, Error> {
        assert_eq!(class.section_len(), entry_len);
        let mut buf = [0_u8; MAX_SECTION_LEN];
        reader.read_exact(&mut buf[..entry_len as usize])?;
        let word_len = class.word_len();
        let slice = &buf[..];
        let name = byte_order.get_u32(slice);
        let slice = &slice[4..];
        let kind: SectionKind = byte_order.get_u32(slice).try_into()?;
        let slice = &slice[4..];
        let flags = Word::new(class, byte_order, slice);
        let slice = &slice[word_len..];
        let virtual_address = Word::new(class, byte_order, slice);
        let slice = &slice[word_len..];
        let offset = Word::new(class, byte_order, slice);
        let slice = &slice[word_len..];
        let size = Word::new(class, byte_order, slice);
        let slice = &slice[word_len..];
        let link = byte_order.get_u32(slice);
        let slice = &slice[4..];
        let info = byte_order.get_u32(slice);
        let slice = &slice[4..];
        let align = Word::new(class, byte_order, slice);
        let slice = &slice[word_len..];
        let entry_len = Word::new(class, byte_order, slice);
        Ok(Self {
            name,
            kind,
            flags: SectionFlags::from_bits_retain(flags.as_u64()),
            virtual_address,
            offset,
            size,
            link,
            info,
            align,
            entry_len,
        })
    }

    pub fn write<W: Write>(
        &self,
        mut writer: W,
        class: Class,
        byte_order: ByteOrder,
        entry_len: u16,
    ) -> Result<(), Error> {
        assert_eq!(class.section_len(), entry_len);
        let mut buf = Vec::with_capacity(entry_len as usize);
        byte_order.write_u32(&mut buf, self.name)?;
        byte_order.write_u32(&mut buf, self.kind.as_u32())?;
        Word::from_u64(class, self.flags.bits())
            .ok_or(Error::InvalidSectionFlags(self.flags.bits()))?
            .write(&mut buf, byte_order)?;
        self.virtual_address.write(&mut buf, byte_order)?;
        self.offset.write(&mut buf, byte_order)?;
        self.size.write(&mut buf, byte_order)?;
        byte_order.write_u32(&mut buf, self.link)?;
        byte_order.write_u32(&mut buf, self.info)?;
        self.align.write(&mut buf, byte_order)?;
        self.entry_len.write(&mut buf, byte_order)?;
        writer.write_all(&buf)?;
        Ok(())
    }

    pub fn read_content<R: Read + Seek>(&self, mut reader: R) -> Result<Vec<u8>, Error> {
        reader.seek(SeekFrom::Start(self.offset.as_u64()))?;
        let mut buf = vec![0_u8; self.size.as_usize()];
        reader.read_exact(&mut buf[..])?;
        Ok(buf)
    }

    pub fn write_content<W: Write + Seek>(
        &mut self,
        writer: W,
        content: &[u8],
        no_overwrite: bool,
    ) -> Result<(), Error> {
        let (offset, size) = store(
            writer,
            self.offset,
            self.size,
            self.align.as_u64(),
            content,
            no_overwrite,
        )?;
        eprintln!(
            "Old offset -> new offset: {:?} -> {:?}",
            self.offset, offset
        );
        eprintln!("Old size -> new size: {:?} -> {:?}", self.size, size);
        eprintln!("Old {:#?}", self);
        self.offset = offset;
        self.size = size;
        Ok(())
    }

    /// Zero out the entry's content.
    pub fn clear_content<W: Write + Seek>(&self, writer: W) -> Result<(), Error> {
        zero(writer, self.offset.as_u64(), self.size.as_u64())
    }

    pub fn validate(&self, program_header: &ProgramHeader) -> Result<(), Error> {
        self.validate_overflow()?;
        self.validate_align()?;
        self.validate_coverage(program_header)?;
        Ok(())
    }

    fn validate_overflow(&self) -> Result<(), Error> {
        if self
            .offset
            .as_u64()
            .checked_add(self.size.as_u64())
            .is_none()
        {
            return Err(Error::TooBig("Section in-file size is too big"));
        }
        if self
            .virtual_address
            .as_u64()
            .checked_add(self.size.as_u64())
            .is_none()
        {
            return Err(Error::TooBig("Section in-memory size is too big"));
        }
        Ok(())
    }

    fn validate_align(&self) -> Result<(), Error> {
        match self.kind {
            SectionKind::NoBits => {
                // BSS section is not stored in the file and has arbitrary offset.
            }
            _ if self.flags.contains(SectionFlags::ALLOC) => {
                let align = self.align.as_u64();
                if align > 1 && self.offset.as_u64() % align != 0
                    || self.virtual_address.as_u64() % self.align.as_u64() != 0
                {
                    let section_start = self.virtual_address.as_u64();
                    let section_end = section_start + self.size.as_u64();
                    return Err(Error::MisalignedSection(section_start, section_end, align));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_coverage(&self, program_header: &ProgramHeader) -> Result<(), Error> {
        // TODO this is quadratic
        let section_start = self.virtual_address.as_u64();
        let section_end = section_start + self.size.as_u64();
        if self.flags.contains(SectionFlags::ALLOC)
            && !program_header.entries.iter().any(|segment| {
                let segment_start = segment.virtual_address.as_u64();
                let segment_end = segment_start + segment.memory_size.as_u64();
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

fn store<W: Write + Seek>(
    mut writer: W,
    old_offset: Word,
    old_size: Word,
    align: u64,
    content: &[u8],
    no_overwrite: bool,
) -> Result<(Word, Word), Error> {
    if content.len() as u64 > old_size.max() {
        return Err(Error::TooBig("Entry content size is too big"));
    }
    let mut offset = old_offset;
    if !no_overwrite && old_size.as_usize() >= content.len() {
        eprintln!(
            "New size fits: {} vs. {}",
            old_size.as_usize(),
            content.len()
        );
        // We have enough space to overwrite the old content.
        writer.seek(SeekFrom::Start(offset.as_u64()))?;
        writer.write_all(content)?;
        // Zero out the remaining old content.
        write_zeroes(&mut writer, old_size.as_u64() - content.len() as u64)?;
    } else {
        eprintln!(
            "Not enough space: {} vs. {}",
            old_size.as_usize(),
            content.len()
        );
        // Not enough space. Have to reallocate.
        let (file_offset, padding) = {
            // Zero alignment means no alignment constraints.
            let align = align.max(1);
            let mut file_offset = writer.seek(SeekFrom::End(0))?;
            let align_remainder = file_offset % align;
            let padding = if align_remainder != 0 {
                align - align_remainder
            } else {
                0
            };
            file_offset += padding;
            assert_eq!(0, file_offset % align);
            if file_offset > old_offset.max() {
                return Err(Error::TooBig("Entry offset is too big"));
            }
            (file_offset, padding)
        };
        write_zeroes(&mut writer, padding)?;
        writer.write_all(content)?;
        // Zero out the old content.
        // TODO
        zero(writer, offset.as_u64(), old_size.as_u64())?;
        offset.set_u64(file_offset).expect("Checked above");
    }
    let mut size = old_size;
    size.set_usize(content.len()).expect("Checked above");
    Ok((offset, size))
}

fn zero<W: Write + Seek>(mut writer: W, offset: u64, size: u64) -> Result<(), Error> {
    writer.seek(SeekFrom::Start(offset))?;
    write_zeroes(writer, size)?;
    Ok(())
}

fn write_zeroes<W: Write + Seek>(mut writer: W, size: u64) -> Result<(), Error> {
    const BUF_LEN: usize = 4096;
    let buf = [0_u8; BUF_LEN];
    for offset in (0..size).step_by(BUF_LEN) {
        let n = (offset + BUF_LEN as u64).min(size) - offset;
        writer.write_all(&buf[..n as usize])?;
    }
    Ok(())
}

fn align_is_valid(align: u64) -> bool {
    align <= 1 || align.checked_next_power_of_two() == Some(align)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;
    use std::fs::OpenOptions;
    use std::io::Cursor;

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
            entry.write_content(&mut file, interpreter, false).unwrap();
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
            .map(|segment| segment.virtual_address.as_u64() + segment.memory_size.as_u64())
            .max()
            .unwrap_or(0)
            .next_multiple_of(MAX_ALIGN as u64);
        let phdr = elf
            .segments
            .get_mut(SegmentKind::ProgramHeader)
            .unwrap()
            .move_to_end(&mut file)
            .unwrap();
        phdr.virtual_address = Word::from_u64(elf.header.class, new_virtual_address).unwrap();
        let phdr_offset = phdr.offset;
        let phdr_addr = phdr.virtual_address;
        let phdr_file_size = phdr.file_size;
        let phdr_memory_size = phdr.memory_size;
        let phdr_align = phdr.align;
        elf.segments.entries.push(Segment {
            kind: SegmentKind::Loadable,
            flags: SegmentFlags::from_bits_retain(1 << 2),
            offset: phdr_offset,
            virtual_address: phdr_addr,
            physical_address: phdr_addr,
            file_size: phdr_file_size,
            memory_size: phdr_memory_size,
            align: phdr_align,
        });
        elf.header.num_segments = elf.segments.entries.len() as u16;
        elf.header.program_header_offset = phdr_offset;
        elf.sections.write(&mut file, &elf.header).unwrap();
        elf.segments.write(&mut file, &elf.header).unwrap();
        elf.header.write(&mut file).unwrap();
    }

    #[test]
    fn program_header_entry_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let entry_len = class.segment_len();
            let expected = Segment::arbitrary(u, class)?;
            let mut buf = Vec::new();
            expected
                .write(&mut buf, class, byte_order, entry_len)
                .unwrap();
            let actual = Segment::read(&buf[..], class, byte_order, entry_len).unwrap();
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
                num_segments: expected.entries.len().try_into().unwrap(),
                segment_len: entry_len,
                program_header_offset: Word::from_u64(class, 0).unwrap(),
                class,
                byte_order,
                os_abi: 0,
                abi_version: 0,
                kind: FileKind::Executable,
                machine: 0,
                flags: 0,
                entry_point: Word::from_u64(class, 0).unwrap(),
                section_header_offset: Word::arbitrary(u, class)?,
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

    #[test]
    fn section_header_entry_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let entry_len = class.section_len();
            let expected = Section::arbitrary(u, class)?;
            let mut buf = Vec::new();
            expected
                .write(&mut buf, class, byte_order, entry_len)
                .unwrap();
            let actual = Section::read(&buf[..], class, byte_order, entry_len).unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    #[test]
    fn section_header_io() {
        arbtest(|u| {
            let class: Class = u.arbitrary()?;
            let byte_order: ByteOrder = u.arbitrary()?;
            let entry_len = class.section_len();
            let expected = SectionHeader::arbitrary(u, class)?;
            let mut cursor = Cursor::new(Vec::new());
            let header = Header {
                num_sections: expected.entries.len().try_into().unwrap(),
                section_len: entry_len,
                section_header_offset: Word::from_u64(class, 0).unwrap(),
                class,
                byte_order,
                os_abi: 0,
                abi_version: 0,
                kind: FileKind::Executable,
                machine: 0,
                flags: 0,
                entry_point: Word::arbitrary(u, class)?,
                program_header_offset: Word::arbitrary(u, class)?,
                segment_len: 0,
                num_segments: 0,
                section_names_index: 0,
                len: class.header_len(),
            };
            expected.write(&mut cursor, &header).unwrap();
            cursor.set_position(0);
            let actual = SectionHeader::read(&mut cursor, &header).unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    #[test]
    fn header_io() {
        arbtest(|u| {
            let expected: Header = u.arbitrary()?;
            let mut cursor = Cursor::new(Vec::new());
            expected
                .write(&mut cursor)
                .inspect_err(|e| panic!("Failed to write {:#?}: {e}", expected))
                .unwrap();
            cursor.set_position(0);
            let actual = Header::read(&mut cursor)
                .inspect_err(|e| panic!("Failed to read {:#?}: {e}", expected))
                .unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    impl<'a> Arbitrary<'a> for Header {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let class: Class = u.arbitrary()?;
            let byte_order = u.arbitrary()?;
            let os_abi = u.arbitrary()?;
            let abi_version = u.arbitrary()?;
            let kind = u.arbitrary()?;
            let machine = u.arbitrary()?;
            let flags = u.arbitrary()?;
            let segment_len = class.segment_len();
            let num_segments = u.int_in_range(0..=100)?;
            let section_len = class.section_len();
            let num_sections = u.int_in_range(0..=100)?;
            let section_names_index = {
                let m = if num_sections == 0 {
                    0
                } else {
                    num_sections - 1
                };
                u.int_in_range(0..=m)?
            };
            let ret = match class {
                Class::Elf32 => Self {
                    class,
                    byte_order,
                    os_abi,
                    abi_version,
                    kind,
                    machine,
                    flags,
                    entry_point: Word::U32(u.arbitrary()?),
                    program_header_offset: Word::U32(u.int_in_range(0..=u32::MAX / 3)?),
                    segment_len,
                    num_segments,
                    section_header_offset: Word::U32(u.int_in_range(u32::MAX / 3 * 2..=u32::MAX)?),
                    section_len,
                    num_sections,
                    section_names_index,
                    len: HEADER_LEN_32 as u16,
                },
                Class::Elf64 => Self {
                    class,
                    byte_order,
                    os_abi,
                    abi_version,
                    kind,
                    machine,
                    flags,
                    entry_point: Word::U64(u.arbitrary()?),
                    program_header_offset: Word::U64(u.int_in_range(0..=u64::MAX / 3)?),
                    segment_len,
                    num_segments,
                    section_header_offset: Word::U64(u.int_in_range(u64::MAX / 3 * 2..=u64::MAX)?),
                    section_len,
                    num_sections,
                    section_names_index,
                    len: HEADER_LEN_64 as u16,
                },
            };
            Ok(ret)
        }
    }

    impl ProgramHeader {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let num_entries = u.arbitrary_len::<[u8; MAX_SEGMENT_LEN]>()?;
            let mut entries: Vec<Segment> = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(Segment::arbitrary(u, class)?);
            }
            Ok(ProgramHeader { entries })
        }
    }

    impl Segment {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let kind = u.arbitrary()?;
            let flags = SegmentFlags::from_bits_retain(u.arbitrary()?);
            let ret = match class {
                Class::Elf32 => Self {
                    kind,
                    flags,
                    offset: Word::U32(u.arbitrary()?),
                    virtual_address: Word::U32(u.arbitrary()?),
                    physical_address: Word::U32(u.arbitrary()?),
                    file_size: Word::U32(u.arbitrary()?),
                    memory_size: Word::U32(u.arbitrary()?),
                    align: Word::U32(1_u32 << u.int_in_range(0..=31)?),
                },
                Class::Elf64 => Self {
                    kind,
                    flags,
                    offset: Word::U64(u.arbitrary()?),
                    virtual_address: Word::U64(u.arbitrary()?),
                    physical_address: Word::U64(u.arbitrary()?),
                    file_size: Word::U64(u.arbitrary()?),
                    memory_size: Word::U64(u.arbitrary()?),
                    align: Word::U64(1_u64 << u.int_in_range(0..=63)?),
                },
            };
            Ok(ret)
        }
    }

    impl SectionHeader {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let num_entries = u.arbitrary_len::<[u8; MAX_SECTION_LEN]>()?;
            let mut entries = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(Section::arbitrary(u, class)?);
            }
            Ok(SectionHeader { entries })
        }
    }

    impl Section {
        fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let name = u.arbitrary()?;
            let link = u.arbitrary()?;
            let info = u.arbitrary()?;
            let kind = u.arbitrary()?;
            let ret = match class {
                Class::Elf32 => Self {
                    name,
                    kind,
                    flags: SectionFlags::from_bits_retain(Word::U32(u.arbitrary()?).as_u64()),
                    virtual_address: Word::U32(u.arbitrary()?),
                    offset: Word::U32(u.arbitrary()?),
                    size: Word::U32(u.arbitrary()?),
                    link,
                    info,
                    align: Word::U32(u.arbitrary()?),
                    entry_len: Word::U32(u.arbitrary()?),
                },
                Class::Elf64 => Self {
                    name,
                    kind,
                    flags: SectionFlags::from_bits_retain(Word::U64(u.arbitrary()?).as_u64()),
                    virtual_address: Word::U64(u.arbitrary()?),
                    offset: Word::U64(u.arbitrary()?),
                    size: Word::U64(u.arbitrary()?),
                    link,
                    info,
                    align: Word::U64(u.arbitrary()?),
                    entry_len: Word::U64(u.arbitrary()?),
                },
            };
            Ok(ret)
        }
    }
}
