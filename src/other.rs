use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;

use crate::Class;
use crate::Error;

pub(crate) fn store<W: Write + Seek>(
    mut writer: W,
    class: Class,
    old_offset: u64,
    old_size: u64,
    align: u64,
    content: &[u8],
    no_overwrite: bool,
) -> Result<(u64, u64), Error> {
    let content_len = content.len() as u64;
    if content_len > class.word_max() {
        return Err(Error::TooBig("Entry content size"));
    }
    let mut offset = old_offset;
    if !no_overwrite && old_size >= content_len {
        eprintln!("New size fits: {} vs. {}", old_size, content.len());
        // We have enough space to overwrite the old content.
        writer.seek(SeekFrom::Start(offset))?;
        writer.write_all(content)?;
        // Zero out the remaining old content.
        write_zeroes(&mut writer, old_size - content_len)?;
    } else {
        eprintln!("Not enough space: {} vs. {}", old_size, content.len());
        // Not enough space. Have to reallocate.
        let (file_offset, padding) = {
            // Zero alignment means no alignment constraints.
            let align = align.max(1);
            let mut file_offset = writer.seek(SeekFrom::End(0))?;
            let align_remainder = file_offset % align;
            let padding = if align_remainder != 0 {
                align - align_remainder
            } else {
                0
            };
            file_offset += padding;
            assert_eq!(0, file_offset % align);
            if file_offset > class.word_max() {
                return Err(Error::TooBig("Entry offset"));
            }
            (file_offset, padding)
        };
        write_zeroes(&mut writer, padding)?;
        writer.write_all(content)?;
        // Zero out the old content.
        zero(writer, offset, old_size)?;
        offset = file_offset;
    }
    Ok((offset, content_len))
}

pub(crate) fn zero<W: Write + Seek>(mut writer: W, offset: u64, size: u64) -> Result<(), Error> {
    writer.seek(SeekFrom::Start(offset))?;
    write_zeroes(writer, size)?;
    Ok(())
}

pub(crate) fn write_zeroes<W: Write + Seek>(mut writer: W, size: u64) -> Result<(), Error> {
    const BUF_LEN: usize = 4096;
    let buf = [0_u8; BUF_LEN];
    for offset in (0..size).step_by(BUF_LEN) {
        let n = (offset + BUF_LEN as u64).min(size) - offset;
        writer.write_all(&buf[..n as usize])?;
    }
    Ok(())
}
