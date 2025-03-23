use std::io::Error;
use std::io::ErrorKind;
use std::io::Write;

use crate::ByteOrder;
use crate::Class;

use ByteOrder::*;
use Class::*;

pub trait ElfRead {
    fn read_u16(&self, byte_order: ByteOrder) -> u16;
    fn read_u32(&self, byte_order: ByteOrder) -> u32;
    fn read_u64(&self, byte_order: ByteOrder) -> u64;
    fn read_word(&self, class: Class, byte_order: ByteOrder) -> u64;

    fn read_i32(&self, byte_order: ByteOrder) -> i32;
    fn read_i64(&self, byte_order: ByteOrder) -> i64;
}

impl ElfRead for [u8] {
    fn read_u16(&self, byte_order: ByteOrder) -> u16 {
        match byte_order {
            LittleEndian => u16::from_le_bytes([self[0], self[1]]),
            BigEndian => u16::from_be_bytes([self[0], self[1]]),
        }
    }

    fn read_u32(&self, byte_order: ByteOrder) -> u32 {
        match byte_order {
            LittleEndian => u32::from_le_bytes([self[0], self[1], self[2], self[3]]),
            BigEndian => u32::from_be_bytes([self[0], self[1], self[2], self[3]]),
        }
    }

    fn read_u64(&self, byte_order: ByteOrder) -> u64 {
        match byte_order {
            LittleEndian => u64::from_le_bytes([
                self[0], self[1], self[2], self[3], self[4], self[5], self[6], self[7],
            ]),
            BigEndian => u64::from_be_bytes([
                self[0], self[1], self[2], self[3], self[4], self[5], self[6], self[7],
            ]),
        }
    }

    fn read_word(&self, class: Class, byte_order: ByteOrder) -> u64 {
        match class {
            Elf32 => self.read_u32(byte_order).into(),
            Elf64 => self.read_u64(byte_order),
        }
    }

    fn read_i32(&self, byte_order: ByteOrder) -> i32 {
        match byte_order {
            LittleEndian => i32::from_le_bytes([self[0], self[1], self[2], self[3]]),
            BigEndian => i32::from_be_bytes([self[0], self[1], self[2], self[3]]),
        }
    }

    fn read_i64(&self, byte_order: ByteOrder) -> i64 {
        match byte_order {
            LittleEndian => i64::from_le_bytes([
                self[0], self[1], self[2], self[3], self[4], self[5], self[6], self[7],
            ]),
            BigEndian => i64::from_be_bytes([
                self[0], self[1], self[2], self[3], self[4], self[5], self[6], self[7],
            ]),
        }
    }
}

pub trait ElfReadMut {
    fn read_u16(&mut self, byte_order: ByteOrder) -> u16;
    fn read_u32(&mut self, byte_order: ByteOrder) -> u32;
    fn read_word(&mut self, class: Class, byte_order: ByteOrder) -> u64;
}

impl ElfReadMut for &[u8] {
    fn read_u16(&mut self, byte_order: ByteOrder) -> u16 {
        let ret = match byte_order {
            LittleEndian => u16::from_le_bytes([self[0], self[1]]),
            BigEndian => u16::from_be_bytes([self[0], self[1]]),
        };
        *self = &self[2..];
        ret
    }

    fn read_u32(&mut self, byte_order: ByteOrder) -> u32 {
        let ret = self[..4].read_u32(byte_order);
        *self = &self[4..];
        ret
    }

    fn read_word(&mut self, class: Class, byte_order: ByteOrder) -> u64 {
        let n = class.word_len();
        let ret = self[..n].read_word(class, byte_order);
        *self = &self[n..];
        ret
    }
}

pub mod v2 {
    use super::*;

    pub trait ElfReadV2 {
        fn read_bytes(&mut self, buf: &mut [u8]) -> Result<(), crate::Error>;

        fn read_u8(&mut self) -> Result<u8, crate::Error> {
            let mut bytes = [0_u8; 1];
            self.read_bytes(&mut bytes[..])?;
            Ok(bytes[0])
        }

        fn read_u16(&mut self, byte_order: ByteOrder) -> Result<u16, crate::Error> {
            let mut bytes = [0_u8; 2];
            self.read_bytes(&mut bytes[..])?;
            let ret = match byte_order {
                LittleEndian => u16::from_le_bytes(bytes),
                BigEndian => u16::from_be_bytes(bytes),
            };
            Ok(ret)
        }

        fn read_u32(&mut self, byte_order: ByteOrder) -> Result<u32, crate::Error> {
            let mut bytes = [0_u8; 4];
            self.read_bytes(&mut bytes[..])?;
            let ret = match byte_order {
                LittleEndian => u32::from_le_bytes(bytes),
                BigEndian => u32::from_be_bytes(bytes),
            };
            Ok(ret)
        }

        fn read_u64(&mut self, byte_order: ByteOrder) -> Result<u64, crate::Error> {
            let mut bytes = [0_u8; 8];
            self.read_bytes(&mut bytes[..])?;
            let ret = match byte_order {
                LittleEndian => u64::from_le_bytes(bytes),
                BigEndian => u64::from_be_bytes(bytes),
            };
            Ok(ret)
        }

        fn read_word(&mut self, class: Class, byte_order: ByteOrder) -> Result<u64, crate::Error> {
            match class {
                Elf32 => self.read_u32(byte_order).map(Into::into),
                Elf64 => self.read_u64(byte_order),
            }
        }
    }

    #[cfg(feature = "std")]
    impl<R: std::io::Read> ElfReadV2 for R {
        fn read_bytes(&mut self, buf: &mut [u8]) -> Result<(), crate::Error> {
            Ok(self.read_exact(buf)?)
        }
    }

    #[cfg(not(feature = "std"))]
    impl ElfReadV2 for &[u8] {
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
                let value: u32 = value.try_into().map_err(|_| ErrorKind::InvalidData)?;
                self.write_u32(byte_order, value)?;
            }
            Elf64 => self.write_u64(byte_order, value)?,
        }
        Ok(())
    }

    fn write_u32_as_u64(&mut self, byte_order: ByteOrder, value: u64) -> Result<(), Error> {
        self.write_u32(
            byte_order,
            value.try_into().map_err(|_| ErrorKind::InvalidData)?,
        )
    }

    fn write_i32_as_i64(&mut self, byte_order: ByteOrder, value: i64) -> Result<(), Error> {
        self.write_i32(
            byte_order,
            value.try_into().map_err(|_| ErrorKind::InvalidData)?,
        )
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Error>;
}

impl<W: Write> ElfWrite for W {
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.write_all(bytes)
    }
}

pub fn get_u16(data: &[u8], byte_order: ByteOrder) -> u16 {
    match byte_order {
        LittleEndian => u16::from_le_bytes([data[0], data[1]]),
        BigEndian => u16::from_be_bytes([data[0], data[1]]),
    }
}

pub fn get_u32(data: &[u8], byte_order: ByteOrder) -> u32 {
    match byte_order {
        LittleEndian => u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        BigEndian => u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
    }
}

pub fn write_u32<W: Write>(mut writer: W, byte_order: ByteOrder, value: u32) -> Result<(), Error> {
    let bytes = match byte_order {
        LittleEndian => value.to_le_bytes(),
        BigEndian => value.to_be_bytes(),
    };
    writer.write_all(&bytes)
}

pub fn get_u64(data: &[u8], byte_order: ByteOrder) -> u64 {
    match byte_order {
        LittleEndian => u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]),
        BigEndian => u64::from_be_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]),
    }
}

pub fn write_u64<W: Write>(mut writer: W, byte_order: ByteOrder, value: u64) -> Result<(), Error> {
    let bytes = match byte_order {
        LittleEndian => value.to_le_bytes(),
        BigEndian => value.to_be_bytes(),
    };
    writer.write_all(&bytes)
}

pub fn get_word(class: Class, byte_order: ByteOrder, data: &[u8]) -> u64 {
    match class {
        Elf32 => get_u32(data, byte_order).into(),
        Elf64 => get_u64(data, byte_order),
    }
}

pub fn write_word<W: Write>(
    writer: W,
    class: Class,
    byte_order: ByteOrder,
    value: u64,
) -> Result<(), Error> {
    match class {
        Elf32 => {
            let value: u32 = value.try_into().map_err(|_| ErrorKind::InvalidData)?;
            write_u32(writer, byte_order, value)?;
        }
        Elf64 => write_u64(writer, byte_order, value)?,
    }
    Ok(())
}

pub fn write_word_u32<W: Write>(
    writer: W,
    class: Class,
    byte_order: ByteOrder,
    value: u32,
) -> Result<(), Error> {
    match class {
        Elf32 => write_u32(writer, byte_order, value),
        Elf64 => write_u64(writer, byte_order, value.into()),
    }
}
