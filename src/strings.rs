use std::ffi::CStr;

pub struct StringTable(Vec<u8>);

impl StringTable {
    pub fn new() -> Self {
        // String tables always start and end with a NUL byte.
        Self(vec![0])
    }

    pub fn insert(&mut self, string: &CStr) -> usize {
        if let Some(offset) = self.get_offset(string) {
            return offset;
        }
        debug_assert!(!self.0.is_empty());
        let offset = self.0.len();
        self.0.extend_from_slice(string.to_bytes_with_nul());
        offset
    }

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

    pub fn get_string(&self, offset: usize) -> Option<&CStr> {
        let c_str_bytes = self.0.get(offset..)?;
        CStr::from_bytes_until_nul(c_str_bytes).ok()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|b| *b == 0)
    }

    /// Get the underlying vector.
    pub fn into_inner(self) -> Vec<u8> {
        self.0
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

#[cfg(test)]
mod tests {
    use super::*;
    use arbtest::arbtest;
    use std::ffi::CString;

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
}
