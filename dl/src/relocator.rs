#![allow(unused)]
#![allow(missing_docs)]
use crate::DynamicLoader;

pub struct ElfRelocator {
    loader: DynamicLoader,
}

impl ElfRelocator {
    pub fn new(loader: DynamicLoader) -> Self {
        Self { loader }
    }
}
