#![doc = include_str!("../README.md")]

mod error;
mod loader;
#[cfg(feature = "relocate")]
mod relocate;

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

pub use self::error::*;
pub use self::loader::*;
#[cfg(feature = "relocate")]
pub use self::relocate::*;
