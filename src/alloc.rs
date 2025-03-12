use crate::constants::*;

#[derive(Default)]
pub struct Allocations {
    allocations: Vec<(u64, Alloc)>,
}

impl Allocations {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, start: u64, end: u64) {
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

    pub fn finish(&mut self) {
        self.push(0, 0);
        self.push(u64::MAX, u64::MAX);
        self.allocations.sort_unstable_by_key(|x| x.0);
    }

    pub fn alloc_file_block(&self, size: u64, memory_offset: u64) -> Option<u64> {
        let mut counter = 0;
        let mut start = None;
        for i in 0..self.allocations.len() {
            let (offset, alloc) = &self.allocations[i];
            match alloc {
                Alloc::Start => counter += 1,
                Alloc::End => counter -= 1,
            }
            if counter == 0 {
                if start.is_none() {
                    start = Some(offset);
                }
            } else {
                if let Some(start) = start {
                    if offset - start >= size {
                        let padding = calc_padding(*start, memory_offset, PAGE_SIZE as u64)?;
                        let padded_size = padding.checked_add(size)?;
                        if offset - start >= padded_size {
                            let start = start.checked_add(padding)?;
                            log::trace!(
                                "Allocating file block {:#x}..{:#x}, padding {}, align {}",
                                start,
                                start + size,
                                padding,
                                PAGE_SIZE,
                            );
                            return Some(start);
                        }
                    }
                }
                start = None;
            }
        }
        None
    }

    pub fn alloc_memory_block(&self, size: u64, align: u64) -> Option<u64> {
        let align = align.max(1);
        let mut counter = 0;
        let mut start = None;
        for i in 0..self.allocations.len() {
            let (offset, alloc) = &self.allocations[i];
            match alloc {
                Alloc::Start => counter += 1,
                Alloc::End => counter -= 1,
            }
            if counter == 0 {
                if start.is_none() {
                    start = Some(offset);
                }
            } else {
                if let Some(start) = start {
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
                start = None;
            }
        }
        None
    }
}

enum Alloc {
    Start,
    End,
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
}
