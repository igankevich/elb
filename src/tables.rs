use bitflags::bitflags;

use crate::define_specific_enum;
use crate::Error;

define_specific_enum! {
    FileKind, u16,
    InvalidFileKind,
    (None, 0),
    (Relocatable, 1),
    (Executable, 2),
    (Shared, 3),
    (Core, 4),
    Range(OsSpecific(0xfe00, 0xfeff)),
    Range(ProcSpecific(0xff00, 0xffff)),
}

impl FileKind {
    pub const fn as_u16(self) -> u16 {
        self.as_number()
    }
}

define_specific_enum! {
    SegmentKind, u32,
    InvalidSegmentKind,
    (Null, 0),
    (Loadable, 1),
    (Dynamic, 2),
    (Interpreter, 3),
    (Note, 4),
    (Shlib, 5),
    (ProgramHeader, 6),
    (Tls, 7),
    Range(OsSpecific(0x60000000, 0x6fffffff)),
    Range(ProcSpecific(0x70000000, 0x7fffffff)),
}

impl SegmentKind {
    pub const fn as_u32(self) -> u32 {
        self.as_number()
    }
}

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct SegmentFlags: u32 {
        const EXECUTABLE = 1 << 0;
        const WRITABLE = 1 << 1;
        const READABLE = 1 << 2;
    }
}

define_specific_enum! {
    DynamicTag, u32,
    InvalidDynamicEntryKind,
    (Null, 0),
    (Needed, 1),
    (PltRelSize, 2),
    (PltGot, 3),
    (Hash, 4),
    (StringTableAddress, 5),
    (SymbolTableAddress, 6),
    (RelaTableAddress, 7),
    (RelaTableSize, 8),
    (RelaEntrySize, 9),
    (StringTableSize, 10),
    (SymbolEntrySize, 11),
    (InitAddress, 12),
    (FiniAddress, 13),
    (SharedObjectName, 14),
    (RpathOffset, 15),
    (Symbolic, 16),
    (RelTableAddress, 17),
    (RelTableSize, 18),
    (RelEntrySize, 19),
    (PltRel, 20),
    (Debug, 21),
    (TextRel, 22),
    (JmpRel, 23),
    (BindNow, 24),
    (InitArray, 25),
    (FiniArray, 26),
    (InitArraySize, 27),
    (FiniArraySize, 28),
    (RunPathOffset, 29),
    (Flags, 30),
    (PreInitArray, 32),
    (PreInitArraySize, 33),
    (SymbolTableIndex, 34),
    (RelrTableSize, 35),
    (RelrTableAddress, 36),
    (RelrEntrySize, 37),
    Range(OsSpecific(0x6000000d, 0x6ffff000)),
    //Range(Other(0x6ffff001, 0x6fffffff)),
    Range(ProcSpecific(0x70000000, 0x7fffffff)),
    Other(Other)
}

impl DynamicTag {
    pub const fn as_u32(self) -> u32 {
        self.as_number()
    }
}

impl TryFrom<u64> for DynamicTag {
    type Error = Error;
    fn try_from(other: u64) -> Result<Self, Self::Error> {
        let number: u32 = other
            .try_into()
            .map_err(|_| Error::TooBig("dynamic-entry-type"))?;
        number.try_into()
    }
}

define_specific_enum! {
    SectionKind, u32,
    InvalidSectionKind,
    (Null, 0),
    (ProgramData, 1),
    (SymbolTable, 2),
    (StringTable, 3),
    (RelaTable, 4),
    (Hash, 5),
    (Dynamic, 6),
    (Note, 7),
    (NoBits, 8),
    (RelTable, 9),
    (Shlib, 10),
    (DynamicSymbolTable, 11),
    (InitArray, 14),
    (FiniArray, 15),
    (PreInitArray, 16),
    (Group, 17),
    (SymbolTableIndex, 18),
    (RelrTable, 19),
    Range(OsSpecific(0x60000000, 0x6fffffff)),
    Range(ProcSpecific(0x70000000, 0x7fffffff)),
    Range(UserSpecific(0x80000000, 0x8fffffff)),
}

impl SectionKind {
    pub const fn as_u32(self) -> u32 {
        self.as_number()
    }
}

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct SectionFlags: u64 {
        const WRITE = 1 << 0;
        const ALLOC = 1 << 1;
        const EXECINSTR = 1 << 2;
        const MERGE = 1 << 4;
        const STRINGS = 1 << 5;
        const INFO_LINK = 1 << 6;
        const LINK_ORDER = 1 << 7;
        const OS_NONCONFORMING = 1 << 8;
        const GROUP = 1 << 9;
        const TLS = 1 << 10;
        const COMPRESSED = 1 << 11;
    }
}
