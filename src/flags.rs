use bitflags::bitflags;

bitflags! {
    /// Segment flags.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct SegmentFlags: u32 {
        const EXECUTABLE = 1 << 0;
        const WRITABLE = 1 << 1;
        const READABLE = 1 << 2;
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
