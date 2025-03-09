use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::io::*;
use crate::ByteOrder;
use crate::Class;
use crate::DynamicEntryKind;
use crate::Error;
use crate::Segment;

#[derive(Debug)]
pub struct DynamicTable {
    entries: Vec<(DynamicEntryKind, u64)>,
}

impl DynamicTable {
    pub fn new() -> Self {
        Self {
            entries: Default::default(),
        }
    }

    // TODO remove
    pub fn read<R: Read + Seek>(
        reader: R,
        segment: &Segment,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<Self, Error> {
        let content = segment.read_content(reader)?;
        let mut slice = &content[..];
        let word_len = class.word_len();
        let step = 2 * word_len;
        let mut entries = Vec::with_capacity(content.len() / step);
        for _ in (0..content.len()).step_by(step) {
            let tag: DynamicEntryKind = get_word(class, byte_order, slice).try_into()?;
            slice = &slice[word_len..];
            let value = get_word(class, byte_order, slice);
            slice = &slice[word_len..];
            entries.push((tag, value));
        }
        Ok(Self { entries })
    }

    pub fn write<W: Write + Seek>(
        &self,
        mut writer: W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        let content = self.to_bytes(class, byte_order)?;
        writer.write_all(&content)?;
        Ok(())
    }

    pub fn from_bytes(content: &[u8], class: Class, byte_order: ByteOrder) -> Result<Self, Error> {
        let mut slice = &content[..];
        let word_len = class.word_len();
        let step = 2 * word_len;
        let mut entries = Vec::with_capacity(content.len() / step);
        for _ in (0..content.len()).step_by(step) {
            let tag: DynamicEntryKind = get_word(class, byte_order, slice).try_into()?;
            slice = &slice[word_len..];
            let value = get_word(class, byte_order, slice);
            slice = &slice[word_len..];
            entries.push((tag, value));
        }
        Ok(Self { entries })
    }

    pub fn to_bytes(&self, class: Class, byte_order: ByteOrder) -> Result<Vec<u8>, Error> {
        let mut content = Vec::new();
        for (kind, value) in self.entries.iter() {
            write_word_u32(&mut content, class, byte_order, kind.as_u32())?;
            write_word(&mut content, class, byte_order, *value)?;
        }
        Ok(content)
    }
}

impl Deref for DynamicTable {
    type Target = Vec<(DynamicEntryKind, u64)>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl DerefMut for DynamicTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}
