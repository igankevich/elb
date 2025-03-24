#![doc = include_str!("../README.md")]
#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod allocations;
mod byte_order;
mod class;
pub(crate) mod constants;
mod dynamic_table;
mod elf;
mod enums;
mod error;
mod flags;
mod header;
mod io;
mod macros;
pub(crate) mod other;
mod patch;
mod relocations;
mod sections;
mod segments;
mod strings;
mod symbols;
pub(crate) mod validation;

pub use self::allocations::*;
pub use self::byte_order::*;
pub use self::class::*;
pub use self::dynamic_table::*;
pub use self::elf::*;
pub use self::enums::*;
pub use self::error::*;
pub use self::flags::*;
pub use self::header::*;
pub use self::io::*;
pub(crate) use self::macros::*;
pub use self::patch::*;
pub use self::relocations::*;
pub use self::sections::*;
pub use self::segments::*;
pub use self::strings::*;
pub use self::symbols::*;
