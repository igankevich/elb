# elb

[![Crates.io Version](https://img.shields.io/crates/v/elb)](https://crates.io/crates/elb)
[![Docs](https://docs.rs/elb/badge.svg)](https://docs.rs/elb)
[![dependency status](https://deps.rs/repo/github/igankevich/elb/status.svg)](https://deps.rs/repo/github/igankevich/elb)

ELF reader/patcher library that features
- reading and writing ELF files,
- patching `RPATH`, `RUNPATH` and interpreter via high-level API,
- verifying correctness of ELF files,
- custom patching via low-level API.

To resolve dependencies without loading and executing files,
you can use [`elb-dl`](https://docs.rs/elb-dl) that is based on this crate.

There is also an accompanying [command-line utility](https://docs.rs/elb-cli).


## Usage

```rust
use elb::{DynamicTag, Elf, ElfPatcher, Error};
use std::fs::{File, OpenOptions};

fn read_elf() -> Result<(), Error> {
    let mut file = File::open("/bin/ls")?;
    let page_size = 4096;
    let elf = Elf::read(&mut file, page_size)?;
    eprintln!("{:#?}", elf.header);
    Ok(())
}

fn patch_elf() -> Result<(), Error> {
    let mut file = OpenOptions::new().read(true).write(true).open("/chroot/bin/ls")?;
    let page_size = 4096;
    let elf = Elf::read(&mut file, page_size)?;
    let mut patcher = ElfPatcher::new(elf, file);
    patcher.set_interpreter(c"/chroot/lib64/ld-linux-x86-64.so.2")?;
    patcher.set_library_search_path(DynamicTag::Runpath, c"/chroot/lib64:/chroot/usr/lib64")?;
    patcher.finish()?;
    Ok(())
}
```
