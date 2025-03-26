use bitflags::bitflags;

bitflags! {
    /// Segment flags.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct SegmentFlags: u32 {
        /// The corresponding memory pages are executable.
        const EXECUTABLE = 1 << 0;
        /// The corresponding memory pages are writable.
        const WRITABLE = 1 << 1;
        /// The corresponding memory pages are readable.
        const READABLE = 1 << 2;
    }
}

bitflags! {
    /// Section flags.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct SectionFlags: u64 {
        /// Writable section.
        ///
        /// Should be placed in a segment that is also [writable](crate::SegmentFlags::WRITABLE).
        const WRITE = 1 << 0;
        /// Allocatable section.
        ///
        /// Should be placed in a [loadable](crate::SegmentKind::Loadable) segment.
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
    /// ARM32-specific flags.
    ///
    /// https://github.com/ARM-software/abi-aa/blob/main/aaelf32/aaelf32.rst
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct ArmFlags: u32 {
        /// Uses software-emulated floating point operations.
        const SOFT_FLOAT = 0x200;
        /// Uses hardware-accelerated floating point operations.
        const HARD_FLOAT = 0x400;
    }
}
