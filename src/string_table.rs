use std::ffi::CStr;

#[derive(Default)]
pub struct StringTable(Vec<u8>);

impl StringTable {
    pub fn insert(&mut self, string: &CStr) -> usize {
        if let Some(offset) = self.get_offset(string) {
            return offset;
        }
        if self.0.is_empty() {
            // String tables always start with NUL byte.
            self.0.push(0);
        }
        let offset = self.0.len();
        self.0.extend_from_slice(string.to_bytes_with_nul());
        offset
    }

    pub fn get_offset(&self, string: &CStr) -> Option<usize> {
        let string = string.to_bytes_with_nul();
        if self.0.is_empty() {
            return None;
        }
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
        let Some(c_str_bytes) = self.0.get(offset..) else {
            return None;
        };
        CStr::from_bytes_until_nul(c_str_bytes).ok()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl From<Vec<u8>> for StringTable {
    fn from(strings: Vec<u8>) -> Self {
        Self(strings)
    }
}

impl AsRef<[u8]> for StringTable {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_offset() {
        assert_eq!(
            Some(0),
            StringTable(b"hello\0".to_vec()).get_offset(c"hello")
        );
        assert_eq!(
            Some(1),
            StringTable(b"\0hello\0".to_vec()).get_offset(c"hello")
        );
        assert_eq!(
            Some(7),
            StringTable(b"\0first\0hello\0".to_vec()).get_offset(c"hello")
        );
        assert_eq!(None, StringTable(b"".to_vec()).get_offset(c"hello"));
        assert_eq!(None, StringTable(b"".to_vec()).get_offset(c""));
        assert_eq!(None, StringTable(b"123".to_vec()).get_offset(c""));
        assert_eq!(Some(0), StringTable(b"\0123".to_vec()).get_offset(c""));
    }

    #[test]
    fn test_symmetry() {
        test_get_string(b"hello\0", c"hello");
        test_get_string(b"\0123", c"");
    }

    fn test_get_string(strings: &[u8], expected: &CStr) {
        let table = StringTable(strings.to_vec());
        let offset = table.get_offset(expected).unwrap();
        let actual = table.get_string(offset).unwrap();
        assert_eq!(expected, actual);
    }
}
