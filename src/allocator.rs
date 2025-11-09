use alloc::vec::Vec;
use core::cmp::Ordering;
use core::cmp::PartialOrd;
use core::ops::Range;

use crate::Class;
use crate::Error;
use crate::Section;
use crate::SectionFlags;
use crate::SectionKind;
use crate::Segment;
use crate::SegmentFlags;
use crate::SegmentKind;

/// Allocator for in-file and in-memory space.
///
/// Allocates sections, segments and raw space.
#[derive(Debug)]
pub struct SpaceAllocator<'a> {
    file_events: Vec<Event>,
    memory_events: Vec<Event>,
    page_size: u64,
    class: Class,
    segments: &'a mut Vec<Segment>,
}

impl<'a> SpaceAllocator<'a> {
    /// Create new allocator for the specified sections and segments.
    pub fn new(
        class: Class,
        page_size: u64,
        sections: &[Section],
        segments: &'a mut Vec<Segment>,
    ) -> Self {
        assert!(page_size > 0 && page_size.is_power_of_two());
        let file_events = Self::file_events(sections, segments);
        let memory_events = Self::memory_events(page_size, sections, segments);
        Self {
            file_events,
            memory_events,
            page_size,
            class,
            segments,
        }
    }

    fn file_events(sections: &[Section], segments: &[Segment]) -> Vec<Event> {
        let mut events = Vec::with_capacity(2 * (sections.len() + segments.len()));
        for (i, section) in sections.iter().enumerate() {
            if matches!(section.kind, SectionKind::Null) {
                continue;
            }
            let range = section.file_offset_range();
            if range.is_empty() {
                continue;
            }
            events.push(Event {
                offset: range.start,
                kind: SectionStart,
                index: i,
            });
            events.push(Event {
                offset: range.end,
                kind: if section.kind == SectionKind::NoBits {
                    NoBitsSectionEnd
                } else {
                    SectionEnd
                },
                index: i,
            });
        }
        for (i, segment) in segments.iter().enumerate() {
            if matches!(segment.kind, SegmentKind::Null) {
                continue;
            }
            let range = segment.file_offset_range();
            events.push(Event {
                offset: range.start,
                kind: if range.is_empty() {
                    EmptySegmentStart
                } else if segment.kind == SegmentKind::Loadable {
                    LoadSegmentStart
                } else {
                    SegmentStart
                },
                index: i,
            });
            events.push(Event {
                offset: range.end,
                kind: if range.is_empty() {
                    EmptySegmentEnd
                } else if segment.kind == SegmentKind::Loadable {
                    LoadSegmentEnd
                } else {
                    SegmentEnd
                },
                index: i,
            });
        }
        events.sort_unstable();
        events
    }

    fn memory_events(page_size: u64, sections: &[Section], segments: &[Segment]) -> Vec<Event> {
        let mut events = Vec::with_capacity(2 * (sections.len() + segments.len()));
        for (i, section) in sections.iter().enumerate() {
            if matches!(section.kind, SectionKind::Null)
                || !section.flags.contains(SectionFlags::ALLOC)
            {
                continue;
            }
            let range = section.virtual_address_range();
            events.push(Event {
                offset: range.start,
                kind: SectionStart,
                index: i,
            });
            events.push(Event {
                offset: range.end,
                kind: SectionEnd,
                index: i,
            });
        }
        for (i, segment) in segments.iter().enumerate() {
            if segment.kind != SegmentKind::Loadable {
                continue;
            }
            // Expand the segment like `ld.so`.
            let range = expand_to_page_boundary(segment.virtual_address_range(), page_size);
            events.push(Event {
                offset: range.start,
                kind: LoadSegmentStart,
                index: i,
            });
            events.push(Event {
                offset: range.end,
                kind: LoadSegmentEnd,
                index: i,
            });
        }
        events.sort_unstable();
        events
    }

    /// Allocate in-file and in-memory space for the specified `ALLOC` section.
    ///
    /// On success sets [`Section::offset`] and [`Section::virtual_address`].
    pub fn allocate_section(mut self, section: &mut Section) -> Result<(), Error> {
        if section.kind == SectionKind::NoBits {
            // TODO handle NoBits
            unimplemented!("Allocating NOBITS sections is not implemented");
        }
        assert!(section.flags.contains(SectionFlags::ALLOC) && section.kind != SectionKind::Null);
        let (offset_from_start, segment_index) = self
            .allocate_space(&self.file_events, section)
            .or_else(|| {
                // We didn't find sufficient free space in existing segments, let's add a new segment.
                self.allocate_loadable_segment_for(
                    section.size,
                    section.size,
                    section.align,
                    segment_flags_for(section.flags),
                )
            })
            .ok_or(Error::SectionAlloc)?;
        let segment = &self.segments[segment_index];
        section.offset = segment.offset + offset_from_start;
        section.virtual_address = segment.virtual_address + offset_from_start;
        Ok(())
    }

    /// Allocate in-file and in-memory space for the specified `LOAD` segment.
    ///
    /// On success sets [`Segment::offset`], [`Segment::virtual_address`] and
    /// [`Segment::physical_address`].
    pub fn allocate_segment(mut self, segment: &mut Segment) -> Result<(), Error> {
        assert!(!matches!(
            segment.kind,
            SegmentKind::Loadable | SegmentKind::Null
        ));
        let (offset_from_start, segment_index) = self
            .allocate_loadable_segment_for(
                segment.file_size,
                segment.memory_size,
                segment.align,
                segment.flags,
            )
            .ok_or(Error::SegmentAlloc)?;
        let outer = &self.segments[segment_index];
        segment.offset = outer.offset + offset_from_start;
        segment.virtual_address = outer.virtual_address + offset_from_start;
        segment.physical_address = outer.physical_address + offset_from_start;
        Ok(())
    }

    fn allocate_loadable_segment_for(
        &mut self,
        file_size: u64,
        memory_size: u64,
        align: u64,
        flags: SegmentFlags,
    ) -> Option<(u64, usize)> {
        let align = align.max(1);
        let offset = self
            .file_events
            .last()
            .map(|event| {
                debug_assert!(matches!(
                    event.kind,
                    LoadSegmentEnd | SegmentEnd | SectionEnd | EmptySegmentEnd
                ));
                event.offset
            })
            .unwrap_or(self.class.header_len() as u64)
            .checked_next_multiple_of(self.page_size)?;
        let virtual_address = self
            .memory_events
            .last()
            .map(|event| {
                debug_assert!(matches!(
                    event.kind,
                    LoadSegmentEnd | SegmentEnd | SectionEnd | EmptySegmentEnd
                ));
                event.offset
            })
            .unwrap_or(0)
            .checked_next_multiple_of(self.page_size)?;
        let padding = {
            let rem = offset % align;
            if rem != 0 {
                align - rem
            } else {
                0
            }
        };
        let file_size = padding.checked_add(file_size)?;
        let memory_size = padding.checked_add(memory_size)?;
        let segment_index = self.segments.len();
        let segment = Segment {
            kind: SegmentKind::Loadable,
            flags,
            offset,
            virtual_address,
            physical_address: virtual_address,
            file_size,
            memory_size,
            align: self.page_size,
        };
        log::trace!(
            "Allocating segment {:?}, file offsets {:#x}..{:#x}, memory offsets {:#x}..{:#x}",
            segment.kind,
            segment.offset,
            segment.offset + segment.file_size,
            segment.virtual_address,
            segment.virtual_address + segment.memory_size
        );
        self.segments.push(segment);
        Some((padding, segment_index))
    }

    fn allocate_space(&self, events: &[Event], section: &Section) -> Option<(u64, usize)> {
        let align = section.align.max(1);
        let mut section_counter = 0;
        let mut segment_counter = 0;
        let mut current_load_segment: Option<usize> = None;
        match events.first().map(|event| event.kind) {
            Some(SectionStart) => section_counter += 1,
            Some(LoadSegmentStart) => {
                current_load_segment = Some(0);
                segment_counter += 1;
            }
            Some(SegmentStart | EmptySegmentStart) => segment_counter += 1,
            // `*Start` events are sorted before `*End` events.
            _ => {
                unreachable!("{events:#?}")
            }
        }
        // Try to find free space in an existing segment to squeeze the new section in.
        for i in 1..events.len() {
            let Event {
                offset,
                kind,
                index,
            } = &events[i];
            if events[i - 1].kind == LoadSegmentEnd && segment_counter == 0 {
                current_load_segment = None;
            }
            match kind {
                LoadSegmentStart => {
                    if segment_counter == 0 && self.segments[*index].is_compatible_with(section) {
                        current_load_segment = Some(*index);
                    }
                    segment_counter += 1;
                }
                SegmentStart | EmptySegmentStart => segment_counter += 1,
                SectionStart => section_counter += 1,
                NoBitsSectionEnd | SectionEnd => section_counter -= 1,
                SegmentEnd | EmptySegmentEnd => segment_counter -= 1,
                LoadSegmentEnd => segment_counter -= 1,
            }
            let Some(current_load_segment) = current_load_segment else {
                // We're not inside the LOAD segment.
                continue;
            };
            let vacant = match (events[i - 1].kind, kind) {
                // We're between the start of the segment and the start of the section.
                (LoadSegmentStart, SectionStart)
                    if segment_counter == 1 && section_counter == 1 =>
                {
                    true
                }
                // We're between the end of the section and the end of the segment.
                (SectionEnd, LoadSegmentEnd) if segment_counter == 0 && section_counter == 0 => {
                    true
                }
                // We're between two sections inside a segment.
                (SectionEnd, SectionStart) if segment_counter == 1 && section_counter == 1 => true,
                _ => false,
            };
            if !vacant {
                continue;
            }
            let start = events[i - 1].offset;
            let rem = start % align;
            let padding = if rem != 0 { align - rem } else { 0 };
            let padded_size = padding.checked_add(section.size)?;
            if offset - start >= padded_size {
                let start = start.checked_add(padding)?;
                let offset_from_start = start - self.segments[current_load_segment].offset;
                return Some((offset_from_start, current_load_segment));
            }
        }
        None
    }

    /// Allocate in-file space of the specified size and alignment in the file.
    ///
    /// Suitable for section header.
    pub fn allocate_file_space(&self, size: u64, align: u64) -> Option<u64> {
        let align = align.max(1);
        let mut counter = 1;
        for i in 1..self.file_events.len() {
            let Event { offset, kind, .. } = &self.file_events[i];
            let prev_counter = counter;
            match kind {
                LoadSegmentStart | SegmentStart | SectionStart | EmptySegmentStart => counter += 1,
                LoadSegmentEnd | SegmentEnd | SectionEnd | NoBitsSectionEnd | EmptySegmentEnd => {
                    counter -= 1
                }
            }
            if !(prev_counter == 0 && counter == 1) {
                // We're not between top-level sections/segments.
                continue;
            }
            let start = self.file_events[i - 1].offset;
            let rem = start % align;
            let padding = if rem != 0 { align - rem } else { 0 };
            let padded_size = padding.checked_add(size)?;
            if offset - start >= padded_size {
                let start = start.checked_add(padding)?;
                return Some(start);
            }
        }
        // Couldn't find the space between existing segments.
        // Allocate the space at the end of the last segment.
        let offset = self
            .file_events
            .last()
            .map(|event| {
                debug_assert!(matches!(
                    event.kind,
                    LoadSegmentEnd | SegmentEnd | SectionEnd
                ));
                event.offset
            })
            .unwrap_or(self.class.header_len() as u64)
            .checked_next_multiple_of(align)?;
        Some(offset)
    }
}

impl core::fmt::Display for SpaceAllocator<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let mut prev_offset = 0;
        for Event { offset, kind, .. } in self.file_events.iter() {
            let event_str = match kind {
                LoadSegmentStart => "[ ",
                LoadSegmentEnd => "] ",
                SegmentStart => "< ",
                SegmentEnd => "> ",
                SectionStart => "( ",
                NoBitsSectionEnd | SectionEnd => ") ",
                EmptySegmentStart => "{ ",
                EmptySegmentEnd => "} ",
            };
            let n = offset - prev_offset;
            if n != 0 {
                write!(f, "{n} ")?;
            }
            f.write_str(event_str)?;
            prev_offset = *offset;
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq, Debug)]
struct Event {
    offset: u64,
    kind: EventKind,
    index: usize,
}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Event {
    fn cmp(&self, other: &Self) -> Ordering {
        self.offset
            .cmp(&other.offset)
            .then_with(|| self.kind.cmp(&other.kind).reverse())
    }
}

// Values control sorting order when offsets are equal.
// - LOAD segments enclose other kinds of segments.
// - Segments enclose sections.
// - Sections enclose nothing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum EventKind {
    LoadSegmentStart = 0,
    SegmentStart = 1,
    SectionStart = 2,
    // Empty segment's event order is reversed.
    EmptySegmentEnd = 3,
    EmptySegmentStart = 4,
    SectionEnd = 5,
    // NOBITS + ALLOC sections can only be at the end of the LOAD segment.
    NoBitsSectionEnd = 6,
    SegmentEnd = 7,
    LoadSegmentEnd = 8,
}

use EventKind::*;

impl Segment {
    fn is_compatible_with(&self, section: &Section) -> bool {
        (self.kind == SegmentKind::Loadable) == section.flags.contains(SectionFlags::ALLOC)
            && self.flags.contains(SegmentFlags::WRITABLE)
                == section.flags.contains(SectionFlags::WRITE)
    }
}

fn segment_flags_for(section_flags: SectionFlags) -> SegmentFlags {
    let mut flags = SegmentFlags::READABLE;
    if section_flags.contains(SectionFlags::WRITE) {
        flags.insert(SegmentFlags::WRITABLE);
    }
    flags
}

pub(crate) const fn align_down(offset: u64, page_size: u64) -> u64 {
    debug_assert!(page_size > 0 && page_size.is_power_of_two());
    offset & !(page_size - 1)
}

pub(crate) const fn align_up(offset: u64, page_size: u64) -> u64 {
    debug_assert!(page_size > 0 && page_size.is_power_of_two());
    let rem = offset & (page_size - 1);
    if rem == 0 {
        return offset;
    }
    offset.saturating_add(page_size - rem)
}

fn expand_to_page_boundary(range: Range<u64>, page_size: u64) -> Range<u64> {
    align_down(range.start, page_size)..align_up(range.end, page_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::vec;
    use arbtest::arbtest;

    use crate::Class;

    #[test]
    fn test_align_down() {
        arbtest(|u| {
            // Test page sizes up to 2 MiB.
            let page_size: u64 = 1_u64 << u.int_in_range(0..=21)?;
            // Page number.
            let i: u64 = u.int_in_range(0..=u64::MAX.div_ceil(page_size))?;
            let offset: u64 = u.int_in_range(0..=page_size - 1)?;
            assert_eq!(
                if offset == 0 {
                    i * page_size
                } else {
                    i.saturating_sub(1) * page_size
                },
                align_down(i * page_size - offset, page_size)
            );
            assert_eq!(i * page_size, align_down(i * page_size, page_size));
            assert_eq!(
                i * page_size,
                align_down((i * page_size).saturating_add(offset), page_size)
            );
            assert_eq!(0, align_down(0, page_size));
            Ok(())
        });
    }

    #[test]
    fn test_align_up() {
        arbtest(|u| {
            // Test page sizes up to 2 MiB.
            let page_size: u64 = 1_u64 << u.int_in_range(0..=21)?;
            // Page number.
            let i: u64 = u.int_in_range(0..=u64::MAX.div_ceil(page_size))?;
            let offset: u64 = if page_size == 1 {
                0
            } else {
                u.int_in_range(0..=page_size - 1)?
            };
            assert_eq!(
                i * page_size,
                align_up((i * page_size).saturating_sub(offset), page_size)
            );
            assert_eq!(i * page_size, align_up(i * page_size, page_size));
            assert_eq!(
                if offset == 0 {
                    i * page_size
                } else {
                    (i + 1).saturating_mul(page_size)
                },
                align_up(i * page_size + offset, page_size)
            );
            assert_eq!(u64::MAX, align_up(u64::MAX, page_size));
            Ok(())
        });
    }

    #[test]
    fn test_align_down_fast() {
        arbtest(|u| {
            let page_size: u64 = 1_u64 << u.int_in_range(0..=21)?;
            let offset: u64 = u.arbitrary()?;
            assert_eq!(
                align_down_naive(offset, page_size),
                align_down(offset, page_size)
            );
            Ok(())
        });
    }

    #[test]
    fn test_align_up_fast() {
        arbtest(|u| {
            let page_size: u64 = 1_u64 << u.int_in_range(1..=21)?;
            let offset: u64 = u.arbitrary()?;
            assert_eq!(
                align_up_naive(offset, page_size),
                align_up(offset, page_size)
            );
            Ok(())
        });
    }

    const fn align_down_naive(offset: u64, page_size: u64) -> u64 {
        offset - offset % page_size
    }

    const fn align_up_naive(offset: u64, page_size: u64) -> u64 {
        let rem = offset % page_size;
        if rem == 0 {
            return offset;
        }
        offset.saturating_add(page_size - rem)
    }

    #[test]
    fn test_allocate_section() {
        // Allocate section at the end of the segment.
        {
            let sections = vec![file_section(1000, 1000, SectionFlags::empty())];
            let mut segments = vec![file_segment(
                1000,
                2000,
                SegmentKind::Loadable,
                SegmentFlags::WRITABLE,
            )];
            let alloc = SpaceAllocator::new(Class::Elf64, 4096, &sections, &mut segments);
            let mut section = section(1000, 1, SectionFlags::WRITE | SectionFlags::ALLOC);
            alloc.allocate_section(&mut section).unwrap();
            assert_eq!(2000, section.offset);
        }
        // Allocate section at the start of the segment.
        {
            let sections = vec![file_section(1000, 1000, SectionFlags::empty())];
            let mut segments = vec![file_segment(
                0,
                2000,
                SegmentKind::Loadable,
                SegmentFlags::WRITABLE,
            )];
            let alloc = SpaceAllocator::new(Class::Elf64, 4096, &sections, &mut segments);
            let mut section = section(1000, 1, SectionFlags::WRITE | SectionFlags::ALLOC);

            alloc.allocate_section(&mut section).unwrap();
            assert_eq!(0, section.offset);
        }
        // Allocate section between two other sections.
        {
            let sections = vec![
                file_section(1000, 1000, SectionFlags::empty()),
                file_section(3000, 1000, SectionFlags::empty()),
            ];
            let mut segments = vec![file_segment(
                1000,
                4000,
                SegmentKind::Loadable,
                SegmentFlags::WRITABLE,
            )];
            let alloc = SpaceAllocator::new(Class::Elf64, 4096, &sections, &mut segments);
            let mut section = section(1000, 1, SectionFlags::WRITE | SectionFlags::ALLOC);
            alloc.allocate_section(&mut section).unwrap();
            assert_eq!(2000, section.offset);
        }
        // Allocate section after the last segment.
        {
            let sections = vec![
                file_section(1000, 1000, SectionFlags::empty()),
                file_section(2000, 1000, SectionFlags::empty()),
            ];
            let mut segments = vec![file_segment(
                1000,
                3000,
                SegmentKind::Loadable,
                SegmentFlags::WRITABLE,
            )];
            let alloc = SpaceAllocator::new(Class::Elf64, 4096, &sections, &mut segments);
            let mut section = section(1000, 1, SectionFlags::WRITE | SectionFlags::ALLOC);
            alloc.allocate_section(&mut section).unwrap();
            assert_eq!(3000, section.offset);
        }
    }

    fn file_section(offset: u64, size: u64, flags: SectionFlags) -> Section {
        Section {
            name_offset: 0,
            kind: SectionKind::ProgramBits,
            flags,
            virtual_address: 0,
            offset,
            size,
            link: 0,
            info: 0,
            align: 1,
            entry_len: 0,
        }
    }

    fn file_segment(offset: u64, size: u64, kind: SegmentKind, flags: SegmentFlags) -> Segment {
        Segment {
            kind,
            flags,
            offset,
            virtual_address: 0,
            physical_address: 0,
            file_size: size,
            memory_size: size,
            align: 1,
        }
    }

    fn section(size: u64, align: u64, flags: SectionFlags) -> Section {
        Section {
            name_offset: 0,
            kind: SectionKind::ProgramBits,
            flags,
            virtual_address: 0,
            offset: 0,
            size,
            link: 0,
            info: 0,
            align,
            entry_len: 0,
        }
    }
}
