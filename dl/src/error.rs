use std::ffi::CString;
use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("ELF error: {0}")]
    Elf(#[from] elfie::Error),
    #[error("Failed to resolve dependency {0:?} of {1:?}")]
    FailedToResolve(CString, PathBuf),
    #[error("Input/output error: {0}")]
    Io(#[from] std::io::Error),
}
