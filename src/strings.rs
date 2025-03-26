use alloc::vec;
use alloc::vec::Vec;
use core::ffi::CStr;

use crate::ElfRead;
use crate::ElfWrite;
use crate::Error;

/// A table that stores NUL-terminated strings.
///
/// Always starts and ends with a NUL byte.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub struct StringTable(Vec<u8>);

impl StringTable {
    /// Create an empty table.
    pub fn new() -> Self {
        // String tables always start and end with a NUL byte.
        Self(vec![0])
    }

    /// Insert new string into the table.
    ///
    /// Does nothing if the string is already in the table.
    ///
    /// Returns the offset at which you can find the string.
    pub fn insert(&mut self, string: &CStr) -> usize {
        if let Some(offset) = self.get_offset(string) {
            return offset;
        }
        debug_assert!(!self.0.is_empty());
        let offset = self.0.len();
        self.0.extend_from_slice(string.to_bytes_with_nul());
        offset
    }

    /// Get the offset of the string in the table.
    ///
    /// Returns `None` if the string isn't present in the table.
    pub fn get_offset(&self, string: &CStr) -> Option<usize> {
        debug_assert!(!self.0.is_empty());
        let string = string.to_bytes_with_nul();
        let mut j = 0;
        let n = string.len();
        for i in 0..self.0.len() {
            if self.0[i] == string[j] {
                j += 1;
                if j == n {
                    return Some(i + 1 - n);
                }
            } else {
                j = 0;
            }
        }
        None
    }

    /// Get a reference to a string at `offset`.
    ///
    /// Returns `None` if the offset is out-of-bounds.
    pub fn get_string(&self, offset: usize) -> Option<&CStr> {
        let c_str_bytes = self.0.get(offset..)?;
        CStr::from_bytes_until_nul(c_str_bytes).ok()
    }

    /// Check that the table contains no strings.
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|b| *b == 0)
    }

    /// Get the underlying byte slice.
    ///
    /// The slice is never empty.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// Get the underlying vector.
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }

    /// Read the table from the `reader`.
    pub fn read<R: ElfRead>(reader: &mut R, len: u64) -> Result<Self, Error> {
        let mut strings = vec![0_u8; len as usize];
        reader.read_bytes(&mut strings[..])?;
        Ok(Self(strings))
    }

    /// Write the table to the `writer`.
    pub fn write<W: ElfWrite>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_bytes(self.as_bytes())
    }
}

impl From<Vec<u8>> for StringTable {
    fn from(mut strings: Vec<u8>) -> Self {
        if strings.is_empty() {
            return Self::new();
        }
        if strings.first().copied() != Some(0) {
            strings.insert(0, 0);
        }
        if strings.last().copied() != Some(0) {
            strings.push(0);
        }
        Self(strings)
    }
}

impl AsRef<[u8]> for StringTable {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Default for StringTable {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: AsRef<CStr>> FromIterator<T> for StringTable {
    fn from_iter<I>(items: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        let mut strings: Vec<u8> = Vec::new();
        strings.push(0_u8);
        for item in items.into_iter() {
            strings.extend_from_slice(item.as_ref().to_bytes_with_nul());
        }
        Self(strings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::ffi::CString;
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;

    use crate::test::test_block_io;
    use crate::BlockIo;
    use crate::ByteOrder;
    use crate::Class;

    #[test]
    fn test_get_offset() {
        assert_eq!(
            Some(1),
            StringTable::from(b"hello\0".to_vec()).get_offset(c"hello")
        );
        assert_eq!(
            Some(1),
            StringTable::from(b"\0hello\0".to_vec()).get_offset(c"hello")
        );
        assert_eq!(
            Some(7),
            StringTable::from(b"\0first\0hello\0".to_vec()).get_offset(c"hello")
        );
        assert_eq!(None, StringTable::from(b"".to_vec()).get_offset(c"hello"));
        assert_eq!(Some(0), StringTable::from(b"".to_vec()).get_offset(c""));
        assert_eq!(Some(0), StringTable::from(b"abc".to_vec()).get_offset(c""));
        assert_eq!(
            Some(0),
            StringTable::from(b"\0abc".to_vec()).get_offset(c"")
        );
    }

    #[test]
    fn test_symmetry() {
        test_get_string(b"hello\0", c"hello");
        test_get_string(b"\0abc", c"");
    }

    fn test_get_string(strings: &[u8], expected: &CStr) {
        let table: StringTable = strings.to_vec().into();
        let offset = table.get_offset(expected).unwrap();
        let actual = table.get_string(offset).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_get_offset_get_string_symmetry() {
        arbtest(|u| {
            let strings: Vec<CString> = u.arbitrary()?;
            let mut table: StringTable = Default::default();
            assert_eq!(Some(0), table.0.last().copied());
            for s in strings.iter() {
                table.insert(s);
                assert_eq!(Some(0), table.0.last().copied());
            }
            for s in strings.iter() {
                let offset = table.get_offset(s).unwrap();
                let actual = table.get_string(offset).unwrap();
                assert_eq!(s.as_ref(), actual);
            }
            Ok(())
        });
    }

    #[test]
    fn string_table_io() {
        test_block_io::<StringTable>();
    }

    impl BlockIo for StringTable {
        fn read<R: ElfRead>(
            reader: &mut R,
            _class: Class,
            _byte_order: ByteOrder,
            len: u64,
        ) -> Result<Self, Error> {
            StringTable::read(reader, len)
        }

        fn write<W: ElfWrite>(
            &self,
            writer: &mut W,
            _class: Class,
            _byte_order: ByteOrder,
        ) -> Result<(), Error> {
            self.write(writer)
        }
    }

    impl<'a> Arbitrary<'a> for StringTable {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let strings: Vec<CString> = u.arbitrary()?;
            Ok(strings.into_iter().collect())
        }
    }
}
