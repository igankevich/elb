use std::ffi::CStr;

pub const MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
pub const VERSION: u8 = 1;

pub const HEADER_LEN_32: usize = 52;
pub const HEADER_LEN_64: usize = 64;
pub const MAX_HEADER_LEN: usize = HEADER_LEN_64;

pub const SEGMENT_LEN_32: usize = 32;
pub const SEGMENT_LEN_64: usize = 56;
pub const MAX_SEGMENT_LEN: usize = SEGMENT_LEN_64;

pub const SECTION_LEN_32: usize = 40;
pub const SECTION_LEN_64: usize = 64;
pub const MAX_SECTION_LEN: usize = SECTION_LEN_64;

pub const PAGE_SIZE: usize = 4096;

pub const INTERP_SECTION: &CStr = c".interp";
pub const SHSTRTAB_SECTION: &CStr = c".shstrtab";
pub const DYNSTR_SECTION: &CStr = c".dynstr";
pub const DYNAMIC_SECTION: &CStr = c".dynamic";

pub const DYNAMIC_ALIGN: u64 = 8;
#[allow(unused)]
pub const PHDR_ALIGN: u64 = 8;
#[allow(unused)]
pub const SECTION_HEADER_ALIGN: u64 = 8;
