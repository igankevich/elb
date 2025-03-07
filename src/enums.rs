#![allow(missing_docs)]

use crate::define_enum_v2;
use crate::define_infallible_enum;
use crate::Error;

define_infallible_enum! {
    "ELF file type.",
    FileKind, u16,
    (None, 0, "Unknown file type."),
    (Relocatable, 1, "Relocatable file."),
    (Executable, 2, "Executable file."),
    (Shared, 3, "Shared object."),
    (Core, 4, "Core dump."),
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
    (Sysv, 0, "UNIX System V."),
    (Hpux, 1, "HP-UX."),
    (Netbsd, 2, "NetBSD."),
    (Gnu, 3, "Linux/GNU."),
    (Solaris, 6, "Solaris."),
    (Aix, 7, "IBM AIX."),
    (Irix, 8, "SGI IRIX."),
    (Freebsd, 9, "FreeBSD."),
    (Tru64, 10, "Compaq TRU64 UNIX."),
    (Modesto, 11, "Novell Modesto."),
    (Openbsd, 12, "OpenBSD."),
    (ArmAeabi, 64, "Arm EABI."),
    (Arm, 97, "Arm."),
    (Standalone, 255, "Standalone (embedded)."),
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
    (None, 0, "Unknown architecture."),
    (M32, 1),
    (Sparc, 2),
    (I386, 3, "Intel 386."),
    (M68k, 4),
    (M88k, 5),
    (Iamcu, 6),
    (I860, 7),
    (Mips, 8, "MIPS."),
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
    (Arm, 40, "Arm 32-bit."),
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
    (X86_64, 62, "AMD x86-64."),
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
    (Aarch64, 183, "Arm 64-bit."),
    (Avr32, 185),
    (Stm8, 186),
    (Tile64, 187),
    (Tilepro, 188),
    (Microblaze, 189),
    (Cuda, 190, "NVIDIA CUDA."),
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
    (Amdgpu, 224, "AMD GPU."),
    (Riscv, 243, "RISC-V."),
    (Bpf, 247, "Linux BPF."),
    (Csky, 252),
    (Loongarch, 258),
}

impl Machine {
    /// Cast to `u16`.
    pub const fn as_u16(self) -> u16 {
        self.as_number()
    }
}

define_infallible_enum! {
    "Segment type.",
    SegmentKind, u32,
    (Null, 0, "Inactive/removed segment."),
    (Loadable, 1, "A segment that is mapped from the file into memory segment on program execution."),
    (Dynamic, 2, "A segment that contains dynamic linking information."),
    (Interpreter, 3, "A segment that contains NUL-terminated interpreter path."),
    (Note, 4, "A segment that contains notes."),
    (Shlib, 5, "Reserved."),
    (ProgramHeader, 6, "A segment that contains program header itself."),
    (Tls, 7, "A segment that contains thread-local storage."),
}

impl SegmentKind {
    /// Cast to `u32`.
    pub const fn as_u32(self) -> u32 {
        self.as_number()
    }
}

define_infallible_enum! {
    "Dynamic table tag.",
    DynamicTag, u32,
    (Null, 0, "End of the table."),
    (Needed, 1, "String table offset to the name of the needed library."),
    (PltRelSize, 2),
    (PltGot, 3),
    (Hash, 4, "The address of the symbol hash table."),
    (StringTableAddress, 5, "The address of the string table."),
    (SymbolTableAddress, 6, "The address of the symbol table."),
    (RelaTableAddress, 7, "The address of the relocation with addends table."),
    (RelaTableSize, 8, "The size in bytes of the relocation with addends table."),
    (RelaEntrySize, 9, "Relocation with addends entry size."),
    (StringTableSize, 10, "The size in bytes of the string table."),
    (SymbolEntrySize, 11, "Symbol table entry size."),
    (InitAddress, 12),
    (FiniAddress, 13),
    (SharedObjectName, 14, "String table offset to the name of the shared object."),
    (Rpath, 15, "String table offset to the library search path."),
    (Symbolic, 16),
    (RelTableAddress, 17, "The address of relocation table."),
    (RelTableSize, 18, "The size in bytes of the relocation table."),
    (RelEntrySize, 19, "Relocation entry size."),
    (PltRel, 20),
    (Debug, 21),
    (TextRel, 22),
    (JmpRel, 23),
    (BindNow, 24),
    (InitArray, 25),
    (FiniArray, 26),
    (InitArraySize, 27),
    (FiniArraySize, 28),
    (Runpath, 29, "String table offset to the library search path."),
    (Flags, 30),
    (PreInitArray, 32),
    (PreInitArraySize, 33),
    (SymbolTableIndex, 34),
    (RelrTableSize, 35, "The size in bytes of the relative relocation table."),
    (RelrTableAddress, 36, "The address of relative relocation table."),
    (RelrEntrySize, 37, "Relative relocation entry size."),
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
        let number: u32 = other.try_into().map_err(|_| Error::TooBig("dynamic-tag"))?;
        Ok(number.into())
    }
}

define_infallible_enum! {
    "Section type.",
    SectionKind, u32,
    (Null, 0, "Inactive/removed section."),
    (ProgramBits, 1, "Program-related data."),
    (SymbolTable, 2, "Symbol table."),
    (StringTable, 3, "String table."),
    (RelaTable, 4, "Relocation entries with addends."),
    (Hash, 5, "Symbol hash table."),
    (Dynamic, 6, "Dynamic linking information."),
    (Note, 7, "Notes."),
    (NoBits, 8, "Same as `ProgramBits` but occupies no space in the file."),
    (RelTable, 9, "Relocation entries without addends."),
    (Shlib, 10, "Reserved."),
    (DynamicSymbolTable, 11, "Dynamic linker symbol table."),
    (InitArray, 14, "Constructors."),
    (FiniArray, 15, "Destructors."),
    (PreInitArray, 16, "Pre-constructors."),
    (Group, 17, "Section group."),
    (SymbolTableIndex, 18, "Extended section indices."),
    (RelrTable, 19, "Relative relocation entries."),
}

impl SectionKind {
    /// Cast to `u32`.
    pub const fn as_u32(self) -> u32 {
        self.as_number()
    }
}

/// Symbol visibility.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[repr(u8)]
pub enum SymbolVisibility {
    /// Default visibility.
    Default = 0,
    /// CPU-specific visibility.
    Internal = 1,
    /// The symbol is not available to other modules.
    Hidden = 2,
    /// The symbol is available to other modules
    /// but local module always resolves to the local symbol.
    Protected = 3,
}

impl SymbolVisibility {
    /// Get visibility from symbol's `other` field.
    pub const fn from_other(other: u8) -> Self {
        match other & 3 {
            0 => Self::Default,
            1 => Self::Internal,
            2 => Self::Hidden,
            3 => Self::Protected,
            _ => unreachable!(),
        }
    }
}

define_enum_v2! {
    "Symbol binding.",
    SymbolBinding, u8,
    (Local, 0, "Local symbol."),
    (Global, 1, "Global symbol."),
    (Weak, 2, "Weak symbol."),
}

impl SymbolBinding {
    /// Cast to `u8`.
    pub const fn as_u8(self) -> u8 {
        self.as_number()
    }

    /// Convert from symbol's `info` field.
    pub fn from_info(info: u8) -> Self {
        Self::from(info >> 4)
    }

    /// Convert to the bits of the symbol's `info` field.
    pub const fn to_info_bits(self) -> u8 {
        self.as_u8() << 4
    }
}

define_enum_v2! {
    "Symbol type.",
    SymbolKind, u8,
    (None, 0, "Unspecified."),
    (Object, 1, "Data object."),
    (Function, 2, "Code object."),
    (Section, 3, "Associated with a section."),
    (File, 4, "File name."),
    (Common, 5, "Common data object."),
    (Tls, 6, "Thread-local data object."),
}

impl SymbolKind {
    /// Cast to `u8`.
    pub const fn as_u8(self) -> u8 {
        self.as_number()
    }

    /// Convert from symbol's `info` field.
    pub fn from_info(info: u8) -> Self {
        Self::from(info & 0xf)
    }

    /// Convert to the bits of the symbol's `info` field.
    pub const fn to_info_bits(self) -> u8 {
        self.as_u8() & 0xf
    }
}
