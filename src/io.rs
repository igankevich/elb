use alloc::vec;
use alloc::vec::Vec;
use core::ffi::CStr;

use crate::ByteOrder;
use crate::Class;
use crate::Error;

use ByteOrder::*;
use Class::*;

macro_rules! define_read {
    ($func: ident, $uint: ident) => {
        #[doc = concat!("Read `", stringify!($uint), "`.")]
        fn $func(&mut self, byte_order: ByteOrder) -> Result<$uint, crate::Error> {
            let mut bytes = [0_u8; ::core::mem::size_of::<$uint>()];
            self.read_bytes(&mut bytes[..])?;
            let ret = match byte_order {
                LittleEndian => $uint::from_le_bytes(bytes),
                BigEndian => $uint::from_be_bytes(bytes),
            };
            Ok(ret)
        }
    };
}

/// ELF-specific read functions.
pub trait ElfRead {
    /// Read enough bytes to fill the buffer `buf`.
    ///
    /// Similar to [`Read::read_exact`](std::io::Read::read_exact).
    fn read_bytes(&mut self, buf: &mut [u8]) -> Result<(), crate::Error>;

    /// Read one byte as `u8`.
    fn read_u8(&mut self) -> Result<u8, crate::Error> {
        let mut bytes = [0_u8; 1];
        self.read_bytes(&mut bytes[..])?;
        Ok(bytes[0])
    }

    /// Read one byte as `i8`.
    fn read_i8(&mut self) -> Result<i8, crate::Error> {
        let mut bytes = [0_u8; 1];
        self.read_bytes(&mut bytes[..])?;
        Ok(bytes[0] as i8)
    }

    define_read!(read_i16, i16);
    define_read!(read_i32, i32);
    define_read!(read_i64, i64);

    define_read!(read_u16, u16);
    define_read!(read_u32, u32);
    define_read!(read_u64, u64);

    /// Read one word.
    ///
    /// Reads `u32` when the class is [`Class::Elf32`], reads `u64` otherwise.
    fn read_word(&mut self, class: Class, byte_order: ByteOrder) -> Result<u64, crate::Error> {
        match class {
            Elf32 => self.read_u32(byte_order).map(Into::into),
            Elf64 => self.read_u64(byte_order),
        }
    }
}

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
impl<R: std::io::Read + ?Sized> ElfRead for R {
    fn read_bytes(&mut self, buf: &mut [u8]) -> Result<(), crate::Error> {
        Ok(self.read_exact(buf)?)
    }
}

#[cfg(not(feature = "std"))]
#[cfg_attr(docsrs, doc(cfg(not(feature = "std"))))]
impl ElfRead for &[u8] {
    fn read_bytes(&mut self, buf: &mut [u8]) -> Result<(), crate::Error> {
        let n = buf.len();
        if n > self.len() {
            return Err(Error::UnexpectedEof);
        }
        buf.copy_from_slice(self[..n]);
        *self = &self[n..];
        Ok(())
    }
}

macro_rules! define_write {
    ($func: ident, $uint: ident) => {
        #[doc = concat!("Write `", stringify!($uint), "`.")]
        fn $func(&mut self, byte_order: ByteOrder, value: $uint) -> Result<(), Error> {
            let bytes = match byte_order {
                LittleEndian => value.to_le_bytes(),
                BigEndian => value.to_be_bytes(),
            };
            self.write_bytes(&bytes)
        }
    };
}

/// ELF-specific write functions.
pub trait ElfWrite {
    /// Write one byte as `u8`.
    fn write_u8(&mut self, value: u8) -> Result<(), Error> {
        self.write_bytes(&[value])
    }

    /// Write one byte as `i8`.
    fn write_i8(&mut self, value: i8) -> Result<(), Error> {
        self.write_bytes(&value.to_ne_bytes())
    }

    define_write!(write_u16, u16);
    define_write!(write_u32, u32);
    define_write!(write_u64, u64);

    define_write!(write_i16, i16);
    define_write!(write_i32, i32);
    define_write!(write_i64, i64);

    /// Write one word.
    ///
    /// Writes `u32` when the class is [`Class::Elf32`], writes `u64` otherwise.
    fn write_word(&mut self, class: Class, byte_order: ByteOrder, value: u64) -> Result<(), Error> {
        match class {
            Elf32 => {
                let value: u32 = value.try_into().map_err(|_| Error::TooBigWord(value))?;
                self.write_u32(byte_order, value)?;
            }
            Elf64 => self.write_u64(byte_order, value)?,
        }
        Ok(())
    }

    /// Write a word specified by `value`.
    fn write_word_as_u32(
        &mut self,
        class: Class,
        byte_order: ByteOrder,
        value: u32,
    ) -> Result<(), Error> {
        match class {
            Elf32 => self.write_u32(byte_order, value),
            Elf64 => self.write_u64(byte_order, value.into()),
        }
    }

    /// Write `value` as `u64`.
    fn write_u32_as_u64(&mut self, byte_order: ByteOrder, value: u64) -> Result<(), Error> {
        self.write_u32(
            byte_order,
            value.try_into().map_err(|_| Error::TooBigWord(value))?,
        )
    }

    /// Write `value` as `i32`.
    fn write_i32_as_i64(&mut self, byte_order: ByteOrder, value: i64) -> Result<(), Error> {
        self.write_i32(
            byte_order,
            value
                .try_into()
                .map_err(|_| Error::TooBigSignedWord(value))?,
        )
    }

    /// Write all bytes.
    ///
    /// Similar to [`Write::write_all`](std::io::Write::write_all).
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Error>;
}

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
impl<W: std::io::Write + ?Sized> ElfWrite for W {
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        Ok(self.write_all(bytes)?)
    }
}

/// ELF-specific seek functions.
pub trait ElfSeek {
    /// Seek to the specified offset from the start of the file.
    fn seek(&mut self, offset: u64) -> Result<(), Error>;
}

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
impl<S: std::io::Seek + ?Sized> ElfSeek for S {
    fn seek(&mut self, offset: u64) -> Result<(), Error> {
        self.seek(std::io::SeekFrom::Start(offset))?;
        Ok(())
    }
}

/// Read an entity from a file or write an entity to a file.
///
/// Usually an entity doesn't occupy the whole section or segment.
pub trait EntityIo {
    /// Read the entity from the `reader`.
    fn read<R: ElfRead>(reader: &mut R, class: Class, byte_order: ByteOrder) -> Result<Self, Error>
    where
        Self: Sized;

    /// Write the entity to the `writer`.
    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error>;
}

/// Read a block of data from a file.
///
/// Usually a block occupies the whole section or segment.
pub trait BlockRead {
    /// Read the table from the `reader`.
    fn read<R: ElfRead>(
        reader: &mut R,
        class: Class,
        byte_order: ByteOrder,
        len: u64,
    ) -> Result<Self, Error>
    where
        Self: Sized;
}

/// Write a block of data to a file.
///
/// Usually a block occupies the whole section or segment.
pub trait BlockWrite {
    /// Write the table to the `writer`.
    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        class: Class,
        byte_order: ByteOrder,
    ) -> Result<(), Error>;
}

impl BlockRead for Vec<u8> {
    fn read<R: ElfRead>(
        reader: &mut R,
        _class: Class,
        _byte_order: ByteOrder,
        len: u64,
    ) -> Result<Self, Error> {
        let n: usize = len.try_into().map_err(|_| Error::TooBig("Block size"))?;
        let mut buf = vec![0_u8; n];
        reader.read_bytes(&mut buf[..])?;
        Ok(buf)
    }
}

impl<T: AsRef<[u8]>> BlockWrite for T {
    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        _class: Class,
        _byte_order: ByteOrder,
    ) -> Result<(), Error> {
        writer.write_bytes(self.as_ref())?;
        Ok(())
    }
}

impl BlockWrite for CStr {
    fn write<W: ElfWrite>(
        &self,
        writer: &mut W,
        _class: Class,
        _byte_order: ByteOrder,
    ) -> Result<(), Error> {
        writer.write_bytes(self.to_bytes_with_nul())?;
        Ok(())
    }
}

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
