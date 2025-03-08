use std::io::Error;
use std::io::ErrorKind;
use std::io::Write;

use crate::ByteOrder;
use crate::Class;

use ByteOrder::*;
use Class::*;

// TODO ElfIo trait?

pub fn get_u16(data: &[u8], byte_order: ByteOrder) -> u16 {
    match byte_order {
        LittleEndian => u16::from_le_bytes([data[0], data[1]]),
        BigEndian => u16::from_be_bytes([data[0], data[1]]),
    }
}

pub fn write_u16<W: Write>(mut writer: W, byte_order: ByteOrder, value: u16) -> Result<(), Error> {
    let bytes = match byte_order {
        LittleEndian => value.to_le_bytes(),
        BigEndian => value.to_be_bytes(),
    };
    writer.write_all(&bytes)
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
