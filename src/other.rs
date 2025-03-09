use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;

use crate::Error;

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
