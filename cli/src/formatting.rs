use elfie::SectionFlags;
use elfie::SectionKind;
use elfie::SegmentFlags;
use elfie::SegmentKind;

pub struct SectionKindStr(pub SectionKind);

impl std::fmt::Display for SectionKindStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use SectionKind::*;
        let s = match self.0 {
            Null => Some("NULL"),
            ProgramData => Some("PROGBITS"),
            SymbolTable => Some("SYMTAB"),
            StringTable => Some("STRTAB"),
            RelaTable => Some("RELA"),
            Hash => Some("HASH"),
            Dynamic => Some("DYNAMIC"),
            Note => Some("NOTE"),
            NoBits => Some("NOBITS"),
            RelTable => Some("REL"),
            Shlib => Some("SHLIB"),
            DynamicSymbolTable => Some("DYNSYM"),
            InitArray => Some("INIT_ARRAY"),
            FiniArray => Some("FINI_ARRAY"),
            PreInitArray => Some("PREINIT_ARRAY"),
            Group => Some("GROUP"),
            SymbolTableIndex => Some("SYMTAB_SHNDX"),
            RelrTable => Some("RELR"),
            OsSpecific(0x6ffffff5) => Some("GNU_ATTRIBUTES"),
            OsSpecific(0x6ffffff6) => Some("GNU_HASH"),
            OsSpecific(0x6ffffff7) => Some("GNU_LIBLIST"),
            OsSpecific(0x6ffffff8) => Some("CHECKSUM"),
            OsSpecific(0x6ffffffd) => Some("GNU_VERDEF"),
            OsSpecific(0x6ffffffe) => Some("GNU_VERNEED"),
            OsSpecific(0x6fffffff) => Some("GNU_VERSYM"),
            _ => None,
        };
        match s {
            Some(s) => write!(f, "{}", s),
            None => write!(f, "{:#x}", self.0.as_u32()),
        }
    }
}

pub struct SegmentKindStr(pub SegmentKind);

impl std::fmt::Display for SegmentKindStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use SegmentKind::*;
        let s = match self.0 {
            Null => Some("NULL"),
            Loadable => Some("LOAD"),
            Dynamic => Some("DYNAMIC"),
            Interpreter => Some("INTERP"),
            Note => Some("NOTE"),
            Shlib => Some("SHLIB"),
            ProgramHeader => Some("PHDR"),
            Tls => Some("TLS"),
            OsSpecific(0x6474e550) => Some("GNU_EH_FRAME"),
            OsSpecific(0x6474e551) => Some("GNU_STACK"),
            OsSpecific(0x6474e552) => Some("GNU_RELRO"),
            OsSpecific(0x6474e553) => Some("GNU_PROPERTY"),
            OsSpecific(0x6474e554) => Some("GNU_SFRAME"),
            _ => None,
        };
        let width = f.width().unwrap_or(0);
        match s {
            Some(s) => write!(f, "{:width$}", s, width = width),
            None => write!(f, "{:#width$x}", self.0.as_u32(), width = width),
        }
    }
}

pub struct SegmentFlagsStr(pub SegmentFlags);

impl std::fmt::Display for SegmentFlagsStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut flags_str = [b'-', b'-', b'-', b' '];
        for flag in self.0.iter() {
            match flag {
                SegmentFlags::READABLE => flags_str[0] = b'r',
                SegmentFlags::WRITABLE => flags_str[1] = b'w',
                SegmentFlags::EXECUTABLE => flags_str[2] = b'x',
                _ => flags_str[3] = b'*',
            }
        }
        let s = std::str::from_utf8(&flags_str[..]).expect("The string is UTF-8");
        write!(f, "{}", s)
    }
}

pub struct SectionFlagsStr(pub SectionFlags);

impl std::fmt::Display for SectionFlagsStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut flags_str = [b'-', b'-', b'-', b'-', b'-', b'-', b'-', b'-', b' '];
        for flag in self.0.iter() {
            match flag {
                SectionFlags::WRITE => flags_str[0] = b'w',
                SectionFlags::ALLOC => flags_str[1] = b'a',
                SectionFlags::EXECINSTR => flags_str[2] = b'x',
                SectionFlags::MERGE => flags_str[3] = b'm',
                SectionFlags::STRINGS => flags_str[4] = b's',
                SectionFlags::INFO_LINK => flags_str[5] = b'i',
                SectionFlags::LINK_ORDER => flags_str[5] = b'l',
                SectionFlags::OS_NONCONFORMING => flags_str[6] = b'o',
                SectionFlags::GROUP => flags_str[6] = b'g',
                SectionFlags::TLS => flags_str[6] = b't',
                SectionFlags::COMPRESSED => flags_str[7] = b'c',
                _ => flags_str[8] = b'*',
            }
        }
        let s = std::str::from_utf8(&flags_str[..]).expect("The string is UTF-8");
        write!(f, "{}", s)
    }
}
