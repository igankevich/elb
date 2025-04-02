#![doc = include_str!("../README.md")]

mod base32;
mod error;
mod loader;
mod relocator;

/// Functionality specific to GNU libc's implementation of the dynamic loader.
#[cfg(feature = "glibc")]
pub mod glibc;
/// Functionality specific to musl libc's implementation of the dynamic loader.
#[cfg(feature = "musl")]
pub mod musl;

#[cfg(feature = "fs-err")]
pub(crate) use fs_err as fs;
#[cfg(not(feature = "fs-err"))]
pub(crate) use std::fs;

pub use self::base32::*;
pub use self::error::*;
pub use self::loader::*;
pub use self::relocator::*;
