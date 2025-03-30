#![doc = include_str!("../README.md")]

mod error;

mod loader;

/// Functionality specific to GNU libc's implementation of the dynamic loader.
#[cfg(feature = "glibc")]
pub mod glibc;
/// Functionality specific to musl libc's implementation of the dynamic loader.
#[cfg(feature = "musl")]
pub mod musl;

pub use self::error::*;
pub use self::loader::*;
