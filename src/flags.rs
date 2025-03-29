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
        // Any bits can be set.
        const _ = !0;
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
        // Any bits can be set.
        const _ = !0;
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
        // Any bits can be set.
        const _ = !0;
    }
}

bitflags! {
    /// RISCV-specific flags.
    ///
    /// https://github.com/riscv-non-isa/riscv-elf-psabi-doc/blob/master/riscv-elf.adoc
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct RiscvFlags: u32 {
        /// Targets C ABI.
        const C_ABI = 0x0001;
        /// Targets E ABI.
        const E_ABI = 0x0008;
        /// Requires RVTSO memory consistency model.
        const TSO = 0x0010;
        /// Requires RV64ILP32 ABI on RV64 ISA.
        const RV64ILP32 = 0x0020;
        // Any bits can be set.
        const _ = !0;
    }
}

impl RiscvFlags {
    /// Get float ABI.
    pub const fn float_abi(self) -> Option<RiscvFloatAbi> {
        match self.bits() & RISCV_FLOAT_ABI_MASK {
            0x0 => Some(RiscvFloatAbi::Soft),
            0x2 => Some(RiscvFloatAbi::Single),
            0x4 => Some(RiscvFloatAbi::Double),
            0x6 => Some(RiscvFloatAbi::Quad),
            _ => None,
        }
    }
}

/// RISCV float ABI.
///
/// Returned by [`RiscvFlags::float_abi`](RiscvFlags::float_abi).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u8)]
pub enum RiscvFloatAbi {
    Soft = 0x0,
    Single = 0x2,
    Double = 0x4,
    Quad = 0x6,
}

const RISCV_FLOAT_ABI_MASK: u32 = 0x6;
