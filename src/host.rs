//! Host parameters (byte order, class, machine).

use crate::ByteOrder;
use crate::Class;
use crate::Machine;

/// Host byte order.
///
/// *NOTE.* Currently Rust supports only little- or big-endian platforms,
/// however, in the future this might change. Byte order will be `None` on such platforms.
pub const BYTE_ORDER: Option<ByteOrder> = if cfg!(target_endian = "little") {
    Some(ByteOrder::LittleEndian)
} else if cfg!(target_endian = "big") {
    Some(ByteOrder::BigEndian)
} else {
    None
};

/// Host class (pointer width).
pub const CLASS: Option<Class> = if cfg!(target_pointer_width = "32") {
    Some(Class::Elf32)
} else if cfg!(target_pointer_width = "64") {
    Some(Class::Elf64)
} else {
    None
};

/// Host machine (architecture).
pub const MACHINE: Option<Machine> = if cfg!(target_arch = "x86_64") {
    Some(Machine::X86_64)
} else if cfg!(target_arch = "arm") {
    Some(Machine::Arm)
} else if cfg!(target_arch = "aarch64") {
    Some(Machine::Aarch64)
} else if cfg!(target_arch = "mips") {
    Some(Machine::Mips)
} else {
    None
};

// TODO ELF flags from target_abi

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Header;
    use fs_err::File;

    #[test]
    fn test_host_params() {
        let mut file = File::open(std::env::current_exe().unwrap()).unwrap();
        let header = Header::read(&mut file).unwrap();
        std::eprintln!("{header:#?}");
        assert_eq!(CLASS, Some(header.class));
        assert_eq!(BYTE_ORDER, Some(header.byte_order));
        assert_eq!(MACHINE, Some(header.machine));
    }
}
