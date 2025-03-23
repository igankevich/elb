use bitflags::bitflags;

use crate::define_infallible_enum;
use crate::define_specific_enum;
use crate::Error;

define_specific_enum! {
    "ELF file type.",
    FileKind, u16,
    InvalidFileKind,
    file_kind_tests,
    (None, 0),
    (Relocatable, 1),
    (Executable, 2),
    (Shared, 3),
    (Core, 4),
    Range(OsSpecific(0xfe00, 0xfeff)),
    Range(ProcSpecific(0xff00, 0xffff)),
}

impl FileKind {
    /// Cast to `u16`.
    pub const fn as_u16(self) -> u16 {
        self.as_number()
    }
}

define_infallible_enum! {
    "Operating system ABI.",
    OsAbi, u8,
    (Sysv, 0),
    (Hpux, 1),
    (Netbsd, 2),
    (Linux, 3),
    (Solaris, 6),
    (Aix, 7),
    (Irix, 8),
    (Freebsd, 9),
    (Tru64, 10),
    (Modesto, 11),
    (Openbsd, 12),
    (ArmAeabi, 64),
    (Arm, 97),
    (Standalone, 255),
}

impl OsAbi {
    /// Cast to `u8`.
    pub const fn as_u8(self) -> u8 {
        self.as_number()
    }
}

define_infallible_enum! {
    "Architecture.",
    Machine, u16,
    (None, 0),
    (M32, 1),
    (Sparc, 2),
    (I386, 3),
    (M68k, 4),
    (M88k, 5),
    (Iamcu, 6),
    (I860, 7),
    (Mips, 8),
    (S370, 9),
    (MipsRs3Le, 10),
    (Parisc, 15),
    (Vpp500, 17),
    (Sparc32plus, 18),
    (I960, 19),
    (Ppc, 20),
    (Ppc64, 21),
    (S390, 22),
    (Spu, 23),
    (V800, 36),
    (Fr20, 37),
    (Rh32, 38),
    (Rce, 39),
    (Arm, 40),
    (FakeAlpha, 41),
    (Sh, 42),
    (Sparcv9, 43),
    (Tricore, 44),
    (Arc, 45),
    (H8300, 46),
    (H8300h, 47),
    (H8s, 48),
    (H8500, 49),
    (Ia64, 50),
    (MipsX, 51),
    (Coldfire, 52),
    (M68hc12, 53),
    (Mma, 54),
    (Pcp, 55),
    (Ncpu, 56),
    (Ndr1, 57),
    (Starcore, 58),
    (Me16, 59),
    (St100, 60),
    (Tinyj, 61),
    (X86_64, 62),
    (Pdsp, 63),
    (Pdp10, 64),
    (Pdp11, 65),
    (Fx66, 66),
    (St9plus, 67),
    (St7, 68),
    (M68hc16, 69),
    (M68hc11, 70),
    (M68hc08, 71),
    (M68hc05, 72),
    (Svx, 73),
    (St19, 74),
    (Vax, 75),
    (Cris, 76),
    (Javelin, 77),
    (Firepath, 78),
    (Zsp, 79),
    (Mmix, 80),
    (Huany, 81),
    (Prism, 82),
    (Avr, 83),
    (Fr30, 84),
    (D10v, 85),
    (D30v, 86),
    (V850, 87),
    (M32r, 88),
    (Mn10300, 89),
    (Mn10200, 90),
    (Pj, 91),
    (Openrisc, 92),
    (ArcCompact, 93),
    (Xtensa, 94),
    (Videocore, 95),
    (TmmGpp, 96),
    (Ns32k, 97),
    (Tpc, 98),
    (Snp1k, 99),
    (St200, 100),
    (Ip2k, 101),
    (Max, 102),
    (Cr, 103),
    (F2mc16, 104),
    (Msp430, 105),
    (Blackfin, 106),
    (SeC33, 107),
    (Sep, 108),
    (Arca, 109),
    (Unicore, 110),
    (Excess, 111),
    (Dxp, 112),
    (AlteraNios2, 113),
    (Crx, 114),
    (Xgate, 115),
    (C166, 116),
    (M16c, 117),
    (Dspic30f, 118),
    (Ce, 119),
    (M32c, 120),
    (Tsk3000, 131),
    (Rs08, 132),
    (Sharc, 133),
    (Ecog2, 134),
    (Score7, 135),
    (Dsp24, 136),
    (Videocore3, 137),
    (Latticemico32, 138),
    (SeC17, 139),
    (TiC6000, 140),
    (TiC2000, 141),
    (TiC5500, 142),
    (TiArp32, 143),
    (TiPru, 144),
    (MmdspPlus, 160),
    (CypressM8c, 161),
    (R32c, 162),
    (Trimedia, 163),
    (Qdsp6, 164),
    (I8051, 165),
    (Stxp7x, 166),
    (Nds32, 167),
    (Ecog1x, 168),
    (Maxq30, 169),
    (Ximo16, 170),
    (Manik, 171),
    (Craynv2, 172),
    (Rx, 173),
    (Metag, 174),
    (McstElbrus, 175),
    (Ecog16, 176),
    (Cr16, 177),
    (Etpu, 178),
    (Sle9x, 179),
    (L10m, 180),
    (K10m, 181),
    (Aarch64, 183),
    (Avr32, 185),
    (Stm8, 186),
    (Tile64, 187),
    (Tilepro, 188),
    (Microblaze, 189),
    (Cuda, 190),
    (Tilegx, 191),
    (Cloudshield, 192),
    (Corea1st, 193),
    (Corea2nd, 194),
    (Arcv2, 195),
    (Open8, 196),
    (Rl78, 197),
    (Videocore5, 198),
    (R78kor, 199),
    (F56800ex, 200),
    (Ba1, 201),
    (Ba2, 202),
    (Xcore, 203),
    (MchpPic, 204),
    (Intelgt, 205),
    (Km32, 210),
    (Kmx32, 211),
    (Emx16, 212),
    (Emx8, 213),
    (Kvarc, 214),
    (Cdp, 215),
    (Coge, 216),
    (Cool, 217),
    (Norc, 218),
    (CsrKalimba, 219),
    (Z80, 220),
    (Visium, 221),
    (Ft32, 222),
    (Moxie, 223),
    (Amdgpu, 224),
    (Riscv, 243),
    (Bpf, 247),
    (Csky, 252),
    (Loongarch, 258),
}

impl Machine {
    /// Cast to `u16`.
    pub const fn as_u16(self) -> u16 {
        self.as_number()
    }
}

define_specific_enum! {
    "Segment type.",
    SegmentKind, u32,
    InvalidSegmentKind,
    segment_kind_tests,
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
    /// Cast to `u32`.
    pub const fn as_u32(self) -> u32 {
        self.as_number()
    }
}

bitflags! {
    /// Segment flags.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct SegmentFlags: u32 {
        const EXECUTABLE = 1 << 0;
        const WRITABLE = 1 << 1;
        const READABLE = 1 << 2;
    }
}

define_specific_enum! {
    "Dynamic table tag.",
    DynamicTag, u32,
    InvalidDynamicEntryKind,
    dynamic_tag_tests,
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
    /// Cast to `u32`.
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
    "Section type.",
    SectionKind, u32,
    InvalidSectionKind,
    section_kind_tests,
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
    /// Cast to `u32`.
    pub const fn as_u32(self) -> u32 {
        self.as_number()
    }
}

bitflags! {
    /// Section flags.
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

bitflags! {
    /// ARM-specific flags.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct ArmFlags: u32 {
        const RELEXEC        = 0x001;
        const HASENTRY       = 0x002;
        const INTERWORK      = 0x004;
        const APCS_26        = 0x008;
        const APCS_FLOAT     = 0x010;
        const PIC            = 0x020;
        const ALIGN8         = 0x040;
        const NEW_ABI        = 0x080;
        const OLD_ABI        = 0x100;
        const SOFT_FLOAT     = 0x200;
        const VFP_FLOAT      = 0x400;
        const MAVERICK_FLOAT = 0x800;
    }
}
