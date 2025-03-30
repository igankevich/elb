#![doc = include_str!("../README.md")]

mod error;

/// Functionality specific to GNU's implementation of the dynamic loader.
#[cfg(feature = "gnu")]
pub mod gnu;
mod loader;

pub use self::error::*;
pub use self::loader::*;
