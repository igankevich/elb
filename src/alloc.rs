#[derive(Default)]
pub struct Allocations {
    allocations: Vec<(u64, Alloc)>,
    page_size: u64,
}

impl Allocations {
    pub fn new(page_size: u64) -> Self {
        assert!(page_size > 0 && page_size.is_power_of_two());
        Self {
            allocations: Default::default(),
            page_size,
        }
    }

    pub fn push(&mut self, start: u64, end: u64) {
        let start = align_down(start, self.page_size);
        let end = align_up(end, self.page_size);
        self.allocations.push((start, Alloc::Start));
        self.allocations.push((end, Alloc::End));
    }

    pub fn extend<I>(&mut self, elements: I)
    where
        I: IntoIterator<Item = (u64, u64)>,
    {
        for (start, end) in elements.into_iter() {
            self.push(start, end);
        }
    }

    pub fn finish(&mut self, min_memory_offset: u64) {
        self.push(min_memory_offset, min_memory_offset);
        self.push(u64::MAX, u64::MAX);
        self.allocations.sort_unstable();
    }

    pub fn alloc_file_block(&self, size: u64, memory_offset: u64) -> Option<u64> {
        let mut counter = 1;
        for i in 1..self.allocations.len() {
            let (offset, alloc) = &self.allocations[i];
            let prev_counter = counter;
            match alloc {
                Alloc::Start => counter += 1,
                Alloc::End => counter -= 1,
            }
            if prev_counter == 0 && counter == 1 {
                let start = self.allocations[i - 1].0;
                if offset - start >= size {
                    let padding = calc_padding(start, memory_offset, self.page_size)?;
                    let padded_size = padding.checked_add(size)?;
                    if offset - start >= padded_size {
                        let start = start.checked_add(padding)?;
                        log::trace!(
                            "Allocating file block {:#x}..{:#x}, padding {}, align {}",
                            start,
                            start + size,
                            padding,
                            self.page_size,
                        );
                        return Some(start);
                    }
                }
            }
        }
        None
    }

    pub fn alloc_memory_block(&self, size: u64, align: u64) -> Option<u64> {
        let align = align.max(1);
        let mut counter = 1;
        for i in 1..self.allocations.len() {
            let (offset, alloc) = &self.allocations[i];
            let prev_counter = counter;
            match alloc {
                Alloc::Start => counter += 1,
                Alloc::End => counter -= 1,
            }
            if prev_counter == 0 && counter == 1 {
                let start = self.allocations[i - 1].0;
                let rem = start % align;
                let padding = if rem != 0 { align - rem } else { 0 };
                let padded_size = padding.checked_add(size)?;
                if offset - start >= padded_size {
                    let start = start.checked_add(padding)?;
                    log::trace!(
                        "Allocating memory block {:#x}..{:#x}, padding {}, align {}",
                        start,
                        start + size,
                        padding,
                        align,
                    );
                    return Some(start);
                }
            }
        }
        None
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Alloc {
    Start = 0,
    End = 1,
}

fn calc_padding(offset1: u64, offset2: u64, align: u64) -> Option<u64> {
    if align <= 1 {
        return Some(0);
    }
    let rem1 = offset1 % align;
    let rem2 = offset2 % align;
    if rem1 != rem2 {
        if rem1 < rem2 {
            let padding = rem2 - rem1;
            offset1.checked_add(padding)?;
            Some(padding)
        } else {
            let padding = (align - rem1).checked_add(rem2)?;
            offset1.checked_add(padding)?;
            Some(padding)
        }
    } else {
        Some(0)
    }
}

const fn align_down(offset: u64, page_size: u64) -> u64 {
    debug_assert!(page_size > 0 && page_size.is_power_of_two());
    offset & !(page_size - 1)
}

const fn align_up(offset: u64, page_size: u64) -> u64 {
    debug_assert!(page_size > 0 && page_size.is_power_of_two());
    let rem = offset & (page_size - 1);
    if rem == 0 {
        return offset;
    }
    offset.saturating_add(page_size - rem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbtest::arbtest;

    #[test]
    fn test_calc_padding() {
        arbtest(|u| {
            let memory_offset = u.arbitrary()?;
            let file_offset = u.arbitrary()?;
            let align = u.arbitrary()?;
            let Some(padding) = calc_padding(memory_offset, file_offset, align) else {
                return Ok(());
            };
            let align = align.max(1);
            assert_eq!((memory_offset + padding) % align, file_offset % align);
            Ok(())
        });
    }

    #[test]
    fn test_alloc_memory_block() {
        let mut allocations = Allocations::new(4096);
        allocations.push(0, 64);
        allocations.push(1000, 2000);
        allocations.push(u64::MAX, u64::MAX);
        allocations.finish(0);
        assert_eq!(Some(2000), allocations.alloc_memory_block(4096, 1000));
    }

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
                    (i + 1).checked_mul(page_size).unwrap_or(u64::MAX)
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
}
