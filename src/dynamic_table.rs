
use std::io::Read;
use std::io::Seek;
use std::io::Write;

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
        writer: W,
        entry: &mut Segment,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
        let mut content = Vec::new();
        for (kind, value) in self.entries.iter() {
            write_word_u32(&mut content, class, byte_order, kind.as_u32())?;
            write_word(&mut content, class, byte_order, *value)?;
        }
        entry.write_content(writer, class, &content, false)?;
        Ok(())
    }

    pub fn get(&self, kind: DynamicEntryKind) -> Option<u64> {
        self.entries
            .iter()
            .find_map(|(k, value)| (*k == kind).then_some(*value))
    }

    pub fn get_mut(&mut self, kind: DynamicEntryKind) -> Option<&mut u64> {
        self.entries
            .iter_mut()
            .find_map(|(k, value)| (*k == kind).then_some(value))
    }

    pub fn push(&mut self, kind: DynamicEntryKind, value: u64) {
        self.entries.push((kind, value));
    }
}
