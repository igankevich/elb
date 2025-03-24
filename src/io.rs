use crate::ByteOrder;
use crate::Class;
use crate::Error;

use ByteOrder::*;
use Class::*;

macro_rules! define_read {
    ($func: ident, $uint: ident) => {
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
    fn read_bytes(&mut self, buf: &mut [u8]) -> Result<(), crate::Error>;

    fn read_u8(&mut self) -> Result<u8, crate::Error> {
        let mut bytes = [0_u8; 1];
        self.read_bytes(&mut bytes[..])?;
        Ok(bytes[0])
    }

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

    fn read_word(&mut self, class: Class, byte_order: ByteOrder) -> Result<u64, crate::Error> {
        match class {
            Elf32 => self.read_u32(byte_order).map(Into::into),
            Elf64 => self.read_u64(byte_order),
        }
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read> ElfRead for R {
    fn read_bytes(&mut self, buf: &mut [u8]) -> Result<(), crate::Error> {
        Ok(self.read_exact(buf)?)
    }
}

#[cfg(not(feature = "std"))]
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
    fn write_u8(&mut self, value: u8) -> Result<(), Error> {
        self.write_bytes(&[value])
    }

    define_write!(write_u16, u16);
    define_write!(write_u32, u32);
    define_write!(write_u64, u64);

    define_write!(write_i32, i32);
    define_write!(write_i64, i64);

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

    fn write_u32_as_u64(&mut self, byte_order: ByteOrder, value: u64) -> Result<(), Error> {
        self.write_u32(
            byte_order,
            value.try_into().map_err(|_| Error::TooBigWord(value))?,
        )
    }

    fn write_i32_as_i64(&mut self, byte_order: ByteOrder, value: i64) -> Result<(), Error> {
        self.write_i32(
            byte_order,
            value
                .try_into()
                .map_err(|_| Error::TooBigSignedWord(value))?,
        )
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Error>;
}

#[cfg(feature = "std")]
impl<W: std::io::Write> ElfWrite for W {
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        Ok(self.write_all(bytes)?)
    }
}

/// ELF-specific seek functions.
pub trait ElfSeek {
    fn seek(&mut self, offset: u64) -> Result<(), Error>;
}

#[cfg(feature = "std")]
impl<S: std::io::Seek> ElfSeek for S {
    fn seek(&mut self, offset: u64) -> Result<(), Error> {
        self.seek(std::io::SeekFrom::Start(offset))?;
        Ok(())
    }
}
