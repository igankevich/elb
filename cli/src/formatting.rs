use elb::DynamicTag;
use elb::SectionFlags;
use elb::SectionKind;
use elb::SegmentFlags;
use elb::SegmentKind;
use elb::SymbolBinding;
use elb::SymbolKind;
use elb::SymbolVisibility;

pub struct SectionKindStr(pub SectionKind);

impl std::fmt::Display for SectionKindStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use SectionKind::*;
        let s = match self.0 {
            Null => Some("NULL"),
            ProgramBits => Some("PROGBITS"),
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
            Other(0x6ffffff5) => Some("GNU_ATTRIBUTES"),
            Other(0x6ffffff6) => Some("GNU_HASH"),
            Other(0x6ffffff7) => Some("GNU_LIBLIST"),
            Other(0x6ffffff8) => Some("CHECKSUM"),
            Other(0x6ffffffd) => Some("GNU_VERDEF"),
            Other(0x6ffffffe) => Some("GNU_VERNEED"),
            Other(0x6fffffff) => Some("GNU_VERSYM"),
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
            Other(0x6474e550) => Some("GNU_EH_FRAME"),
            Other(0x6474e551) => Some("GNU_STACK"),
            Other(0x6474e552) => Some("GNU_RELRO"),
            Other(0x6474e553) => Some("GNU_PROPERTY"),
            Other(0x6474e554) => Some("GNU_SFRAME"),
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
                SectionFlags::EXECUTABLE => flags_str[2] = b'x',
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

pub struct SymbolVisibilityStr(pub SymbolVisibility);

impl std::fmt::Display for SymbolVisibilityStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use SymbolVisibility::*;
        let s = match self.0 {
            Default => "default",
            Internal => "internal",
            Hidden => "hidden",
            Protected => "protected",
        };
        let width = f.width().unwrap_or(0);
        write!(f, "{:width$}", s, width = width)
    }
}

pub struct SymbolBindingStr(pub SymbolBinding);

impl std::fmt::Display for SymbolBindingStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use SymbolBinding::*;
        let s = match self.0 {
            Local => Some("local"),
            Global => Some("global"),
            Weak => Some("weak"),
            _ => None,
        };
        let width = f.width().unwrap_or(0);
        match s {
            Some(s) => write!(f, "{:width$}", s, width = width),
            None => write!(f, "{:#width$x}", self.0.as_u8(), width = width),
        }
    }
}

pub struct SymbolKindStr(pub SymbolKind);

impl std::fmt::Display for SymbolKindStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = match self.0 {
            SymbolKind::None => Some(""),
            SymbolKind::Object => Some("object"),
            SymbolKind::Function => Some("function"),
            SymbolKind::Section => Some("section"),
            SymbolKind::File => Some("file"),
            SymbolKind::Common => Some("common"),
            SymbolKind::Tls => Some("tls"),
            _ => None,
        };
        let width = f.width().unwrap_or(0);
        match s {
            Some(s) => write!(f, "{:width$}", s, width = width),
            None => write!(f, "{:#width$x}", self.0.as_u8(), width = width),
        }
    }
}

pub struct DynamicTagStr(pub DynamicTag);

impl std::fmt::Display for DynamicTagStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = match self.0 {
            DynamicTag::Needed => Some("NEEDED"),
            DynamicTag::PltRelSize => Some("PLTRELSZ"),
            DynamicTag::PltGot => Some("PLTGOT"),
            DynamicTag::Hash => Some("HASH"),
            DynamicTag::StringTableAddress => Some("STRTAB"),
            DynamicTag::SymbolTableAddress => Some("SYMTAB"),
            DynamicTag::RelaTableAddress => Some("RELA"),
            DynamicTag::RelaTableSize => Some("RELASZ"),
            DynamicTag::RelaEntrySize => Some("RELAENT"),
            DynamicTag::StringTableSize => Some("STRSZ"),
            DynamicTag::SymbolEntrySize => Some("SYMSZ"),
            DynamicTag::InitAddress => Some("INIT"),
            DynamicTag::FiniAddress => Some("FINI"),
            DynamicTag::SharedObjectName => Some("SONAME"),
            DynamicTag::Rpath => Some("RPATH"),
            DynamicTag::Symbolic => Some("SYMBOLIC"),
            DynamicTag::RelTableAddress => Some("REL"),
            DynamicTag::RelTableSize => Some("RELSZ"),
            DynamicTag::RelEntrySize => Some("RELENT"),
            DynamicTag::PltRel => Some("PLTREL"),
            DynamicTag::Debug => Some("DEBUG"),
            DynamicTag::TextRel => Some("TEXTREL"),
            DynamicTag::JmpRel => Some("JMPREL"),
            DynamicTag::BindNow => Some("BIND_NOW"),
            DynamicTag::InitArray => Some("INIT_ARRAY"),
            DynamicTag::InitArraySize => Some("INIT_ARRAYSZ"),
            DynamicTag::FiniArray => Some("FINI_ARRAY"),
            DynamicTag::FiniArraySize => Some("FINI_ARRAYSZ"),
            DynamicTag::Runpath => Some("RUNPATH"),
            DynamicTag::Flags => Some("FLAGS"),
            DynamicTag::PreInitArray => Some("PREINIT_ARRAY"),
            DynamicTag::PreInitArraySize => Some("PREINIT_ARRAYSZ"),
            DynamicTag::SymbolTableIndex => Some("SYMTAB_SHNDX"),
            DynamicTag::RelrTableAddress => Some("RELR"),
            DynamicTag::RelrTableSize => Some("RELRSZ"),
            DynamicTag::RelrEntrySize => Some("RELRENT"),
            DynamicTag::Other(0x6ffffef5) => Some("GNU_HASH"),
            DynamicTag::Other(0x6ffffffe) => Some("VERNEED"),
            DynamicTag::Other(0x6fffffff) => Some("VERNEEDNUM"),
            DynamicTag::Other(0x6ffffff0) => Some("VERSYM"),
            DynamicTag::Other(0x6ffffff9) => Some("RELACOUNT"),
            _ => None,
        };
        let width = f.width().unwrap_or(0);
        match s {
            Some(s) => write!(f, "{:width$}", s, width = width),
            None => write!(f, "{:#width$x}", self.0.as_u32(), width = width),
        }
    }
}
