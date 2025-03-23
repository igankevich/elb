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

    pub fn from_bytes(content: &[u8], class: Class, byte_order: ByteOrder) -> Result<Self, Error> {
        let mut slice = content;
        let word_len = class.word_len();
        let step = 2 * word_len;
        let mut entries = Vec::with_capacity(content.len() / step);
        for _ in (0..content.len()).step_by(step) {
            let tag: DynamicTag = get_word(class, byte_order, slice).try_into()?;
            if tag == DynamicTag::Null {
                // NULL entry marks the end of the section.
                break;
            }
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
        // Write NULL to mark the end of the section.
        write_word_u32(&mut content, class, byte_order, 0)?;
        write_word_u32(&mut content, class, byte_order, 0)?;
        Ok(content)
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
