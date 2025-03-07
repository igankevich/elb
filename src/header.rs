use core::ops::Range;

use crate::check_u32;
use crate::constants::*;
use crate::ByteOrder;
use crate::Class;
use crate::ElfRead;
use crate::ElfWrite;
use crate::Error;
use crate::FileKind;
use crate::Machine;
use crate::OsAbi;

/// ELF header.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Header {
    /// Bitness.
    pub class: Class,
    /// Data format.
    pub byte_order: ByteOrder,
    /// Operating system ABI.
    pub os_abi: OsAbi,
    /// ABI version
    pub abi_version: u8,
    /// File type.
    pub kind: FileKind,
    /// Architecture.
    pub machine: Machine,
    /// Architecture-specific flags.
    ///
    /// Use [`ArmFlags`](crate::ArmFlags) to query ARM-specific flags.
    pub flags: u32,
    /// Program entry point.
    pub entry_point: u64,
    /// Program header (the list of segments) offset within the file.
    pub program_header_offset: u64,
    /// The length of each segment's metadata entry.
    pub segment_len: u16,
    /// The number of segments.
    pub num_segments: u16,
    /// Section header (the list of sections) offset within the file.
    pub section_header_offset: u64,
    /// The length of each section's metadata entry.
    pub section_len: u16,
    /// The number of sections.
    pub num_sections: u16,
    /// The index of the section in the section header that stores the names of sections.
    pub section_names_index: u16,
    /// The length of the ELF header.
    pub len: u16,
}

impl Header {
    /// Read header from `reader`.
    pub fn read<R: ElfRead>(reader: &mut R) -> Result<Self, Error> {
        let mut magic = [0_u8; MAGIC.len()];
        reader.read_bytes(&mut magic[..]).map_err(|e| match e {
            Error::UnexpectedEof => Error::NotElf,
            e => e,
        })?;
        if magic != MAGIC {
            return Err(Error::NotElf);
        }
        let class: Class = reader.read_u8()?.try_into()?;
        let byte_order: ByteOrder = reader.read_u8()?.try_into()?;
        let version = reader.read_u8()?;
        if version != VERSION {
            return Err(Error::InvalidVersion(version));
        }
        let os_abi = reader.read_u8()?.into();
        let abi_version = reader.read_u8()?;
        reader.read_bytes(&mut [0_u8; 7])?;
        let kind: FileKind = reader.read_u16(byte_order)?.into();
        let machine = reader.read_u16(byte_order)?.into();
        let version = reader.read_u8()?;
        if version != VERSION {
            return Err(Error::InvalidVersion(version));
        }
        reader.read_bytes(&mut [0_u8; 3])?;
        let entry_point = reader.read_word(class, byte_order)?;
        let program_header_offset = reader.read_word(class, byte_order)?;
        let section_header_offset = reader.read_word(class, byte_order)?;
        let flags = reader.read_u32(byte_order)?;
        let real_header_len = reader.read_u16(byte_order)?;
        let segment_len = reader.read_u16(byte_order)?;
        let num_segments = reader.read_u16(byte_order)?;
        let section_len = reader.read_u16(byte_order)?;
        let num_sections = reader.read_u16(byte_order)?;
        let section_names_index = reader.read_u16(byte_order)?;
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

    /// Write header to `writer`.
    ///
    /// The header is validated before writing.
    pub fn write<W: ElfWrite>(&self, writer: &mut W) -> Result<(), Error> {
        self.check()?;
        writer.write_bytes(&MAGIC)?;
        writer.write_u8(self.class as u8)?;
        writer.write_u8(self.byte_order as u8)?;
        writer.write_u8(VERSION)?;
        writer.write_u8(self.os_abi.as_u8())?;
        writer.write_u8(self.abi_version)?;
        writer.write_bytes(&[0_u8; 7])?;
        writer.write_u16(self.byte_order, self.kind.as_u16())?;
        writer.write_u16(self.byte_order, self.machine.as_u16())?;
        writer.write_u8(VERSION)?;
        writer.write_bytes(&[0_u8; 3])?;
        writer.write_word(self.class, self.byte_order, self.entry_point)?;
        writer.write_word(self.class, self.byte_order, self.program_header_offset)?;
        writer.write_word(self.class, self.byte_order, self.section_header_offset)?;
        writer.write_u32(self.byte_order, self.flags)?;
        writer.write_u16(self.byte_order, self.len)?;
        writer.write_u16(self.byte_order, self.segment_len)?;
        writer.write_u16(self.byte_order, self.num_segments)?;
        writer.write_u16(self.byte_order, self.section_len)?;
        writer.write_u16(self.byte_order, self.num_sections)?;
        writer.write_u16(self.byte_order, self.section_names_index)?;
        Ok(())
    }

    /// Validate the header.
    pub fn check(&self) -> Result<(), Error> {
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
                check_u32(self.entry_point, "Entry point")?;
                check_u32(self.program_header_offset, "Program header offset")?;
                check_u32(self.section_header_offset, "Section header offset")?;
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
        if self.section_names_index != 0
            && self.num_sections != 0
            && self.section_names_index > self.num_sections
        {
            return Err(Error::InvalidSectionHeaderStringTableIndex(
                self.section_names_index,
            ));
        }
        Ok(())
    }

    /// The size in bytes of the program header (the list of segments).
    pub const fn program_header_len(&self) -> u64 {
        self.segment_len as u64 * self.num_segments as u64
    }

    /// The size in bytes of the section header (the list of sections).
    pub const fn section_header_len(&self) -> u64 {
        self.section_len as u64 * self.num_sections as u64
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

    use alloc::vec::Vec;
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
