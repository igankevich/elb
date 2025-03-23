#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

mod alloc;
mod byte_order;
mod class;
pub(crate) mod constants;
mod dynamic_table;
mod elf;
mod error;
mod header;
mod io;
mod macros;
pub(crate) mod other;
mod relocations;
mod sections;
mod segments;
mod strings;
mod symbols;
mod tables;
pub(crate) mod validation;

pub use self::alloc::*;
pub use self::byte_order::*;
pub use self::class::*;
pub use self::dynamic_table::*;
pub use self::elf::*;
pub use self::error::*;
pub use self::header::*;
pub use self::io::*;
pub(crate) use self::macros::*;
pub use self::relocations::*;
pub use self::sections::*;
pub use self::segments::*;
pub use self::strings::*;
pub use self::symbols::*;
pub use self::tables::*;
