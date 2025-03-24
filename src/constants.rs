use core::ffi::CStr;

pub const MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
pub const VERSION: u8 = 1;

pub const HEADER_LEN_32: usize = 52;
pub const HEADER_LEN_64: usize = 64;

pub const SEGMENT_LEN_32: usize = 32;
pub const SEGMENT_LEN_64: usize = 56;

pub const SECTION_LEN_32: usize = 40;
pub const SECTION_LEN_64: usize = 64;

pub const DYNAMIC_LEN_32: usize = 8;
pub const DYNAMIC_LEN_64: usize = 16;

pub const SYMBOL_LEN_32: usize = 16;
pub const SYMBOL_LEN_64: usize = 24;

pub const REL_LEN_32: usize = 8;
pub const REL_LEN_64: usize = 16;

pub const RELA_LEN_32: usize = 12;
pub const RELA_LEN_64: usize = 24;

pub const SECTION_RESERVED_MIN: usize = 0xff00;
pub const SECTION_RESERVED_MAX: usize = 0xffff;

pub const DEFAULT_PAGE_SIZE: u64 = 4096;

pub const INTERP_SECTION: &CStr = c".interp";
pub const SHSTRTAB_SECTION: &CStr = c".shstrtab";
pub const DYNSTR_SECTION: &CStr = c".dynstr";
pub const DYNAMIC_SECTION: &CStr = c".dynamic";
pub const SYMTAB_SECTION: &CStr = c".symtab";

#[allow(unused)]
pub const DYNAMIC_ALIGN: u64 = 8;
pub const DYNAMIC_ENTRY_LEN: u64 = 16;
#[allow(unused)]
pub const PHDR_ALIGN: u64 = 8;
#[allow(unused)]
pub const SECTION_HEADER_ALIGN: u64 = 8;
