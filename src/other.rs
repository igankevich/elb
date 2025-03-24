use crate::ElfSeek;
use crate::ElfWrite;
use crate::Error;

pub(crate) fn zero<W: ElfWrite + ElfSeek>(
    writer: &mut W,
    offset: u64,
    size: u64,
) -> Result<(), Error> {
    writer.seek(offset)?;
    write_zeroes(writer, size)?;
    Ok(())
}

pub(crate) fn write_zeroes<W: ElfWrite + ElfSeek>(writer: &mut W, size: u64) -> Result<(), Error> {
    const BUF_LEN: usize = 4096;
    let buf = [0_u8; BUF_LEN];
    for offset in (0..size).step_by(BUF_LEN) {
        let n = (offset + BUF_LEN as u64).min(size) - offset;
        writer.write_bytes(&buf[..n as usize])?;
    }
    Ok(())
}
