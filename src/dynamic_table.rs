use alloc::vec::Vec;
use core::ffi::CStr;
use core::ops::Deref;
use core::ops::DerefMut;

use crate::io::*;
use crate::BlockRead;
use crate::BlockWrite;
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
    /// Create empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// The on-disk size of the table in bytes.
    pub fn in_file_len(&self, class: Class) -> usize {
        let x = if self.entries.last() == Some(&(DynamicTag::Null, 0)) {
            0
        } else {
            1
        };
        (self.entries.len() + x) * class.dynamic_len()
    }
}

impl BlockRead for DynamicTable {
    fn read<R: ElfRead>(
        reader: &mut R,
        class: Class,
        byte_order: ByteOrder,
        len: u64,
    ) -> Result<Self, Error> {
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
}

impl BlockWrite for DynamicTable {
    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error> {
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
}

impl DynamicTable {
    /// Set table entry under key `tag` to value `value`.
    ///
    /// If the key matches multiple entries, only the first matched entry is updated, and all the subsequent
    /// entries are removed from the table.
    ///
    /// Panics if the `tag` is [`NULL`](crate::DynamicTag::Null).
    pub fn set(&mut self, tag: DynamicTag, value: u64) {
        assert_ne!(DynamicTag::Null, tag);
        match self.entries.iter().position(|(t, _)| *t == tag) {
            Some(i) => {
                log::trace!("Replacing dynamic table entry {tag:?} at index {i} with {value}");
                // Set to NULL temporarily.
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

    /// Get the value associated with the specified tag.
    ///
    /// Returns the first value if there are multiple values in the table.
    pub fn get(&self, tag: DynamicTag) -> Option<u64> {
        self.iter()
            .find_map(|(kind, value)| (*kind == tag).then_some(*value))
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

/// Dynamic table entry's value.
pub enum DynamicValue<'a> {
    /// C-string.
    CStr(&'a CStr),
    /// Word.
    Word(u64),
}

impl<'a> From<&'a CStr> for DynamicValue<'a> {
    fn from(other: &'a CStr) -> Self {
        Self::CStr(other)
    }
}

impl From<u64> for DynamicValue<'_> {
    fn from(other: u64) -> Self {
        Self::Word(other)
    }
}
