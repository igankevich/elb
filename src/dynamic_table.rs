use std::ops::Deref;
use std::ops::DerefMut;

use crate::io::*;
use crate::ByteOrder;
use crate::Class;
use crate::DynamicTag;
use crate::Error;

/// Dynamic linking information.
#[derive(Default, Debug)]
pub struct DynamicTable {
    entries: Vec<(DynamicTag, u64)>,
}

impl DynamicTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn in_file_len(&self, class: Class) -> usize {
        let x = if self.entries.last() == Some(&(DynamicTag::Null, 0)) {
            0
        } else {
            1
        };
        (self.entries.len() + x) * class.dynamic_len()
    }

    pub fn read<R: ElfRead>(
        mut reader: R,
        class: Class,
        byte_order: ByteOrder,
        len: u64,
    ) -> Result<Self, Error>
    where
        for<'a> &'a mut R: ElfRead,
    {
        let mut entries = Vec::with_capacity((len / class.dynamic_len() as u64) as usize);
        let step = class.dynamic_len();
        for _ in (0..len).step_by(step) {
            let tag: DynamicTag = reader.read_word(class, byte_order)?.try_into()?;
            if tag == DynamicTag::Null {
                // NULL entry marks the end of the section.
                break;
            }
            let value = reader.read_word(class, byte_order)?;
            entries.push((tag, value));
        }
        Ok(Self { entries })
    }

    pub fn write<W: ElfWrite>(
        &self,
        mut writer: W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error>
    where
        for<'a> &'a mut W: ElfWrite,
    {
        for (kind, value) in self.entries.iter() {
            writer.write_word_as_u32(class, byte_order, kind.as_u32())?;
            writer.write_word(class, byte_order, *value)?;
        }
        if self.entries.last() != Some(&(DynamicTag::Null, 0)) {
            // Write NULL entry to mark the end of the section.
            writer.write_word_as_u32(class, byte_order, 0)?;
            writer.write_word_as_u32(class, byte_order, 0)?;
        }
        Ok(())
    }

    pub fn set(&mut self, tag: DynamicTag, value: u64) {
        assert_ne!(DynamicTag::Null, tag);
        match self.entries.iter().position(|(t, _)| *t == tag) {
            Some(i) => {
                log::trace!("Replacing dynamic table entry {tag:?} at index {i} with {value}");
                // Set to NULL temporarily
                self.entries[i].0 = DynamicTag::Null;
                self.entries[i].1 = value;
                // Remove other values if any.
                self.entries.retain(|(t, _)| *t != tag);
                // Set proper tag.
                self.entries[i].0 = tag;
            }
            None => {
                log::trace!("Adding dynamic table entry {tag:?} to {value}");
                self.entries.push((tag, value));
            }
        }
    }
}

impl Deref for DynamicTable {
    type Target = Vec<(DynamicTag, u64)>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl DerefMut for DynamicTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}
