#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod error;
mod loader;
#[cfg(feature = "relocate")]
#[cfg_attr(docsrs, doc(cfg(feature = "relocate")))]
mod relocate;

/// Functionality specific to GNU libc's implementation of the dynamic loader.
#[cfg(feature = "glibc")]
#[cfg_attr(docsrs, doc(cfg(feature = "glibc")))]
pub mod glibc;
/// Functionality specific to musl libc's implementation of the dynamic loader.
#[cfg(feature = "musl")]
#[cfg_attr(docsrs, doc(cfg(feature = "musl")))]
pub mod musl;

#[cfg(feature = "fs-err")]
pub(crate) use fs_err as fs;
#[cfg(not(feature = "fs-err"))]
pub(crate) use std::fs;

pub use self::error::*;
pub use self::loader::*;
#[cfg(feature = "relocate")]
#[cfg_attr(docsrs, doc(cfg(feature = "relocate")))]
pub use self::relocate::*;
