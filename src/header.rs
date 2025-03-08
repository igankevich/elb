use std::io::ErrorKind::UnexpectedEof;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::ops::Range;

use crate::constants::*;
use crate::io::*;
use crate::validation::*;
use crate::ByteOrder;
use crate::Class;
use crate::Error;
use crate::FileKind;

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
    pub entry_point: u64,
    pub program_header_offset: u64,
    pub segment_len: u16,
    pub num_segments: u16,
    pub section_header_offset: u64,
    pub section_len: u16,
    pub num_sections: u16,
    pub section_names_index: u16,
    pub len: u16,
}

impl Header {
    pub fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let mut buf = [0_u8; MAX_HEADER_LEN];
        reader.read_exact(&mut buf[..5]).map_err(|e| {
            if e.kind() == UnexpectedEof {
                return Error::NotElf;
            }
            e.into()
        })?;
        if buf[..MAGIC.len()] != MAGIC {
            return Err(Error::NotElf);
        }
        let class: Class = buf[4].try_into()?;
        let header_len = class.header_len();
        reader.read_exact(&mut buf[5..header_len as usize])?;
        let byte_order: ByteOrder = buf[5].try_into()?;
        let version = buf[6];
        if version != VERSION {
            return Err(Error::InvalidVersion(version));
        }
        let os_abi = buf[7];
        let abi_version = buf[8];
        let kind: FileKind = get_u16(&buf[16..18], byte_order).try_into()?;
        let machine = get_u16(&buf[18..20], byte_order);
        let version = buf[20];
        if version != VERSION {
            return Err(Error::InvalidVersion(version));
        }
        let word_len = class.word_len();
        let entry_point = get_word(class, byte_order, &buf[24..]);
        let slice = &buf[24 + word_len..];
        let program_header_offset = get_word(class, byte_order, slice);
        let slice = &slice[word_len..];
        let section_header_offset = get_word(class, byte_order, slice);
        let slice = &slice[word_len..];
        let flags = get_u32(slice, byte_order);
        let slice = &slice[4..];
        let real_header_len = get_u16(slice, byte_order);
        let slice = &slice[2..];
        let segment_len = get_u16(slice, byte_order);
        let slice = &slice[2..];
        let num_segments = get_u16(slice, byte_order);
        let slice = &slice[2..];
        let section_len = get_u16(slice, byte_order);
        let slice = &slice[2..];
        let num_sections = get_u16(slice, byte_order);
        let slice = &slice[2..];
        let section_names_index = get_u16(slice, byte_order);
        if real_header_len > header_len {
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
        write_u16(&mut buf[16..], self.byte_order, self.kind.as_u16())?;
        write_u16(&mut buf[18..], self.byte_order, self.machine)?;
        buf[20] = VERSION;
        let word_len = self.class.word_len();
        let mut offset = 24;
        write_word(
            &mut buf[offset..],
            self.class,
            self.byte_order,
            self.entry_point,
        )?;
        offset += word_len;
        write_word(
            &mut buf[offset..],
            self.class,
            self.byte_order,
            self.program_header_offset,
        )?;
        offset += word_len;
        write_word(
            &mut buf[offset..],
            self.class,
            self.byte_order,
            self.section_header_offset,
        )?;
        offset += word_len;
        write_u32(&mut buf[offset..], self.byte_order, self.flags)?;
        offset += 4;
        write_u16(&mut buf[offset..], self.byte_order, self.len)?;
        offset += 2;
        write_u16(&mut buf[offset..], self.byte_order, self.segment_len)?;
        offset += 2;
        write_u16(&mut buf[offset..], self.byte_order, self.num_segments)?;
        offset += 2;
        write_u16(&mut buf[offset..], self.byte_order, self.section_len)?;
        offset += 2;
        write_u16(&mut buf[offset..], self.byte_order, self.num_sections)?;
        offset += 2;
        write_u16(
            &mut buf[offset..],
            self.byte_order,
            self.section_names_index,
        )?;
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&buf[..self.len as usize])?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.len != self.class.header_len() {
            return Err(Error::InvalidHeaderLen(self.len));
        }
        if self.section_len != 0 && self.section_len != self.class.section_len() {
            return Err(Error::InvalidSectionLen(self.section_len));
        }
        if self.segment_len != 0 && self.segment_len != self.class.segment_len() {
            return Err(Error::InvalidSegmentLen(self.segment_len));
        }
        let (segments_range, sections_range) = match self.class {
            Class::Elf32 => {
                validate_u32(self.entry_point, "Entry point")?;
                validate_u32(self.program_header_offset, "Program header offset")?;
                validate_u32(self.section_header_offset, "Section header offset")?;
                let segments_start = self.program_header_offset as u32;
                let segments_end = (self.segment_len as u32)
                    .checked_mul(self.num_segments.into())
                    .ok_or(Error::TooBig("No. of segments"))?
                    .checked_add(segments_start)
                    .ok_or(Error::TooBig("No. of segments"))?;
                let sections_start = self.section_header_offset as u32;
                let sections_end = (self.segment_len as u32)
                    .checked_mul(self.num_sections.into())
                    .ok_or(Error::TooBig("No. of sections"))?
                    .checked_add(sections_start)
                    .ok_or(Error::TooBig("No. of sections"))?;
                let segments_range = segments_start as u64..segments_end as u64;
                let sections_range = sections_start as u64..sections_end as u64;
                (segments_range, sections_range)
            }
            Class::Elf64 => {
                let segments_start = self.program_header_offset;
                let segments_end = (self.segment_len as u64)
                    .checked_mul(self.num_segments.into())
                    .ok_or(Error::TooBig("No. of segments"))?
                    .checked_add(segments_start)
                    .ok_or(Error::TooBig("No. of segments"))?;
                let sections_start = self.section_header_offset;
                let sections_end = (self.segment_len as u64)
                    .checked_mul(self.num_sections.into())
                    .ok_or(Error::TooBig("No. of sections"))?
                    .checked_add(sections_start)
                    .ok_or(Error::TooBig("No. of sections"))?;
                let segments_range = segments_start..segments_end;
                let sections_range = sections_start..sections_end;
                (segments_range, sections_range)
            }
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;

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
                    entry_point: u.arbitrary::<u32>()?.into(),
                    program_header_offset: u.int_in_range(0..=u32::MAX / 3)?.into(),
                    segment_len,
                    num_segments,
                    section_header_offset: u.int_in_range(u32::MAX / 3 * 2..=u32::MAX)?.into(),
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
                    entry_point: u.arbitrary()?,
                    program_header_offset: u.int_in_range(0..=u64::MAX / 3)?,
                    segment_len,
                    num_segments,
                    section_header_offset: u.int_in_range(u64::MAX / 3 * 2..=u64::MAX)?,
                    section_len,
                    num_sections,
                    section_names_index,
                    len: HEADER_LEN_64 as u16,
                },
            };
            Ok(ret)
        }
    }
}
