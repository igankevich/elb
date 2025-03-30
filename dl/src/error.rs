use std::ffi::CString;
use std::path::PathBuf;

/// Dynamic loader error.
#[derive(thiserror::Error, Debug)]
#[allow(missing_docs)]
pub enum Error {
    #[error("ELF error: {0}")]
    Elf(#[from] elb::Error),
    #[error("Failed to resolve dependency {0:?} of {1:?}")]
    FailedToResolve(CString, PathBuf),
    #[error("Input/output error: {0}")]
    Io(#[from] std::io::Error),
}
