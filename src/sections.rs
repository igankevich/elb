use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::constants::*;
use crate::io::*;
use crate::other::*;
use crate::validation::*;
use crate::ByteOrder;
use crate::Class;
use crate::Error;
use crate::Header;
use crate::ProgramHeader;
use crate::SectionFlags;
use crate::SectionKind;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct SectionHeader {
    entries: Vec<Section>,
}

impl SectionHeader {
    pub fn read<R: Read + Seek>(mut reader: R, header: &Header) -> Result<Self, Error> {
        reader.seek(SeekFrom::Start(header.section_header_offset))?;
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
        writer.seek(SeekFrom::Start(header.section_header_offset))?;
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

    pub fn validate(&self, class: Class, program_header: &ProgramHeader) -> Result<(), Error> {
        for section in self.entries.iter() {
            section.validate(class, program_header)?;
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
    pub virtual_address: u64,
    pub offset: u64,
    pub size: u64,
    pub link: u32,
    pub info: u32,
    pub align: u64,
    pub entry_len: u64,
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
        let name = get_u32(slice, byte_order);
        let slice = &slice[4..];
        let kind: SectionKind = get_u32(slice, byte_order).try_into()?;
        let slice = &slice[4..];
        let flags = get_word(class, byte_order, slice);
        let slice = &slice[word_len..];
        let virtual_address = get_word(class, byte_order, slice);
        let slice = &slice[word_len..];
        let offset = get_word(class, byte_order, slice);
        let slice = &slice[word_len..];
        let size = get_word(class, byte_order, slice);
        let slice = &slice[word_len..];
        let link = get_u32(slice, byte_order);
        let slice = &slice[4..];
        let info = get_u32(slice, byte_order);
        let slice = &slice[4..];
        let align = get_word(class, byte_order, slice);
        let slice = &slice[word_len..];
        let entry_len = get_word(class, byte_order, slice);
        Ok(Self {
            name,
            kind,
            flags: SectionFlags::from_bits_retain(flags),
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
        write_u32(&mut buf, byte_order, self.name)?;
        write_u32(&mut buf, byte_order, self.kind.as_u32())?;
        write_word(&mut buf, class, byte_order, self.flags.bits())?;
        write_word(&mut buf, class, byte_order, self.virtual_address)?;
        write_word(&mut buf, class, byte_order, self.offset)?;
        write_word(&mut buf, class, byte_order, self.size)?;
        write_u32(&mut buf, byte_order, self.link)?;
        write_u32(&mut buf, byte_order, self.info)?;
        write_word(&mut buf, class, byte_order, self.align)?;
        write_word(&mut buf, class, byte_order, self.entry_len)?;
        writer.write_all(&buf)?;
        Ok(())
    }

    pub fn read_content<R: Read + Seek>(&self, mut reader: R) -> Result<Vec<u8>, Error> {
        reader.seek(SeekFrom::Start(self.offset))?;
        let n: usize = self
            .size
            .try_into()
            .map_err(|_| Error::TooBig("Section size"))?;
        let mut buf = vec![0_u8; n];
        reader.read_exact(&mut buf[..])?;
        Ok(buf)
    }

    pub fn write_content<W: Write + Seek>(
        &mut self,
        writer: W,
        class: Class,
        content: &[u8],
        no_overwrite: bool,
    ) -> Result<(), Error> {
        let (offset, size) = store(
            writer,
            class,
            self.offset,
            self.size,
            self.align,
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
        zero(writer, self.offset, self.size)
    }

    pub fn validate(&self, class: Class, program_header: &ProgramHeader) -> Result<(), Error> {
        self.validate_overflow(class)?;
        self.validate_align()?;
        self.validate_coverage(program_header)?;
        Ok(())
    }

    fn validate_overflow(&self, class: Class) -> Result<(), Error> {
        match class {
            Class::Elf32 => {
                validate_u32(self.flags.bits(), "Section flags")?;
                validate_u32(self.virtual_address, "Section virtual address")?;
                validate_u32(self.offset, "Section offset")?;
                validate_u32(self.size, "Section size")?;
                validate_u32(self.align, "Section align")?;
                validate_u32(self.entry_len, "Section entry size")?;
            }
            Class::Elf64 => {
                if self.offset.checked_add(self.size).is_none() {
                    return Err(Error::TooBig("Section in-file size"));
                }
                if self.virtual_address.checked_add(self.size).is_none() {
                    return Err(Error::TooBig("Section in-memory size"));
                }
            }
        }
        Ok(())
    }

    fn validate_align(&self) -> Result<(), Error> {
        match self.kind {
            SectionKind::NoBits => {
                // BSS section is not stored in the file and has arbitrary offset.
            }
            _ if self.flags.contains(SectionFlags::ALLOC) => {
                let align = self.align;
                if align > 1 && self.offset % align != 0 || self.virtual_address % self.align != 0 {
                    let section_start = self.virtual_address;
                    let section_end = section_start + self.size;
                    return Err(Error::MisalignedSection(section_start, section_end, align));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_coverage(&self, program_header: &ProgramHeader) -> Result<(), Error> {
        // TODO this is quadratic
        let section_start = self.virtual_address;
        let section_end = section_start + self.size;
        if self.flags.contains(SectionFlags::ALLOC)
            && !program_header.iter().any(|segment| {
                let segment_start = segment.virtual_address;
                let segment_end = segment_start + segment.memory_size;
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    use arbitrary::Unstructured;
    use arbtest::arbtest;

    use crate::FileKind;

    #[test]
    fn section_io() {
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
                section_header_offset: 0,
                class,
                byte_order,
                os_abi: 0,
                abi_version: 0,
                kind: FileKind::Executable,
                machine: 0,
                flags: 0,
                entry_point: class.arbitrary_word(u)?,
                program_header_offset: class.arbitrary_word(u)?,
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

    impl SectionHeader {
        pub fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            let num_entries = u.arbitrary_len::<[u8; MAX_SECTION_LEN]>()?;
            let mut entries = Vec::with_capacity(num_entries);
            for _ in 0..num_entries {
                entries.push(Section::arbitrary(u, class)?);
            }
            Ok(SectionHeader { entries })
        }
    }

    impl Section {
        pub fn arbitrary(u: &mut Unstructured<'_>, class: Class) -> arbitrary::Result<Self> {
            Ok(Self {
                name: u.arbitrary()?,
                kind: u.arbitrary()?,
                flags: SectionFlags::from_bits_retain(class.arbitrary_word(u)?),
                virtual_address: class.arbitrary_word(u)?,
                offset: class.arbitrary_word(u)?,
                size: class.arbitrary_word(u)?,
                link: u.arbitrary()?,
                info: u.arbitrary()?,
                align: class.arbitrary_align(u)?,
                entry_len: class.section_len().into(),
            })
        }
    }
}
