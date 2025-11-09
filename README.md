# elb

[![Crates.io Version](https://img.shields.io/crates/v/elb)](https://crates.io/crates/elb)
[![Docs](https://docs.rs/elb/badge.svg)](https://docs.rs/elb)
[![dependency status](https://deps.rs/repo/github/igankevich/elb/status.svg)](https://deps.rs/repo/github/igankevich/elb)

ELF reader/patcher library that features
- reading and writing ELF files,
- patching `RPATH`, `RUNPATH`, `SONAME` and interpreter via high-level API,
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
    patcher.set_dynamic_tag(DynamicTag::Runpath, c"/chroot/lib64:/chroot/usr/lib64")?;
    patcher.finish()?;
    Ok(())
}
```


## References

Other ELF readers/patchers:
- [NixOS `patchelf`](https://github.com/NixOS/patchelf).
- [LIEF](https://github.com/lief-project/LIEF).
- [Linux `binfmt_elf`](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/fs/binfmt_elf.c).

Dynamic linkers:
- [Glibc](https://sourceware.org/git/?p=glibc.git;a=blob;f=elf/dl-load.c;h=6b7e9799f323d04bf47a672bb28c99e477808e85;hb=HEAD#l1149).
- [Musl](https://git.musl-libc.org/cgit/musl/tree/ldso/dynlink.c#n685).

Man pages:
- [`elf(5)`](https://man7.org/linux/man-pages/man5/elf.5.html).

Linters:
- ELFUTILS [`eu-elflint`](https://sourceware.org/elfutils/).
- Binutils [`readelf --lint`](https://sourceware.org/git/?p=binutils.git;a=blob;f=binutils/readelf.c;h=7920100630038dfba93ae9bf2c4d4a9bfaa17bde;hb=HEAD).
