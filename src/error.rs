use thiserror::Error;

use crate::SectionKind;
use crate::SegmentKind;

/// ELF-specific error.
#[derive(Error)]
#[allow(missing_docs)]
pub enum Error {
    #[error("Not an ELF file")]
    NotElf,
    #[error("Invalid ELF class: {0}")]
    InvalidClass(u8),
    #[error("Invalid byte order: {0}")]
    InvalidByteOrder(u8),
    #[error("Invalid version: {0}")]
    InvalidVersion(u8),
    #[error("Invalid ELF header size: {0}")]
    InvalidHeaderLen(u16),
    #[error("Invalid section header string table index: {0}")]
    InvalidSectionHeaderStringTableIndex(u16),
    #[error("Invalid entry point: {0:#x}")]
    InvalidEntryPoint(u64),
    #[error("Invalid PHDR segment: {0}")]
    InvalidProgramHeaderSegment(&'static str),
    #[error("Invalid file kind: {0}")]
    InvalidFileKind(u16),
    #[error("Invalid segment kind: {0}")]
    InvalidSegmentKind(u32),
    #[error("Invalid segment size: {0}")]
    InvalidSegmentLen(u16),
    #[error("Invalid section kind: {0}")]
    InvalidSectionKind(u32),
    #[error("Invalid section size: {0}")]
    InvalidSectionLen(u16),
    #[error("Invalid first section kind: {0:?} (should be NULL)")]
    InvalidFirstSectionKind(SectionKind),
    #[error("Too many sections: {0}")]
    TooManySections(usize),
    #[error("Invalid ALLOC section: should be covered by LOAD segment: {0:#x}..{1:#x}")]
    SectionNotCovered(u64, u64),
    #[error("Invalid dynamic entry kind: {0:#x}")]
    InvalidDynamicEntryKind(u32),
    #[error("Invalid alignment kind: {0}")]
    InvalidAlign(u64),
    #[error(
        "Misaligned segment: file offsets range = {0:#x}..{1:#x}, \
        memory addresses range = {2:#x}..{3:#x}, alignment = {4}"
    )]
    MisalignedSegment(u64, u64, u64, u64, u64),
    #[error("Misaligned section: memory addresses range = {0:#x}..{1:#x}, alignment = {2}")]
    MisalignedSection(u64, u64, u64),
    #[error("Segments overlap: {0:#x}..{1:#x}, {2:#x}..{3:#x}")]
    SegmentsOverlap(u64, u64, u64, u64),
    #[error("LOAD segments are not sorted by virtual address")]
    SegmentsNotSorted,
    #[error("Segment {0:?} should preceed any LOAD segment")]
    NotPreceedingLoadSegment(SegmentKind),
    #[error("Only one {0:?} segment is allowed")]
    MultipleSegments(SegmentKind),
    #[error("Only one {0:?} section is allowed")]
    MultipleSections(SectionKind),
    #[error("Overflow: {0}")]
    TooBig(&'static str),
    #[error("Word overflow: {0}")]
    TooBigWord(u64),
    #[error("Signed word overflow: {0}")]
    TooBigSignedWord(i64),
    #[error("Overlap: {0}")]
    Overlap(&'static str),
    #[error("Failed to allocate new section")]
    SectionAlloc,
    #[error("Failed to allocate new segment")]
    SegmentAlloc,
    #[error("Failed to allocate in-file space")]
    FileSpaceAlloc,
    #[error("Input/output error: {0}")]
    #[cfg(feature = "std")]
    Io(std::io::Error),
    #[error("Invalid C-string")]
    CStr,
    #[error("Unexpected EOF")]
    UnexpectedEof,
}

#[cfg(feature = "std")]
impl From<std::io::Error> for Error {
    fn from(other: std::io::Error) -> Self {
        if other.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::UnexpectedEof
        } else {
            Self::Io(other)
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::ErrorKind> for Error {
    fn from(other: std::io::ErrorKind) -> Self {
        if other == std::io::ErrorKind::UnexpectedEof {
            Self::UnexpectedEof
        } else {
            Self::Io(other.into())
        }
    }
}

impl From<alloc::ffi::FromVecWithNulError> for Error {
    fn from(_other: alloc::ffi::FromVecWithNulError) -> Self {
        Self::CStr
    }
}

impl core::fmt::Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

pub(crate) fn check_u32(word: u64, name: &'static str) -> Result<(), Error> {
    if word > u32::MAX as u64 {
        return Err(Error::TooBig(name));
    }
    Ok(())
}
