# elb-cli

[![Crates.io Version](https://img.shields.io/crates/v/elb-cli)](https://crates.io/crates/elb-cli)
[![Docs](https://docs.rs/elb-cli/badge.svg)](https://docs.rs/elb-cli)
[![dependency status](https://deps.rs/repo/github/igankevich/elb-cli/status.svg)](https://deps.rs/repo/github/igankevich/elb-cli)

Command-line utility that inspects ELF files, prints their dependencies and patches RPATH, RUNPATH and interpreter.

Based on [`elb`](https://docs.rs/elb) crate.


## Installation

```sh
cargo install elb-cli
```


## Usage


### Show header/sections/segments/tables

```sh
$ cargo install elb-cli

$ elb-cli show -t header /bin/sh
Class: Elf64
Byte order: LittleEndian
OS ABI: Sysv
ABI version: 0
File type: Executable
Machine: X86_64
Flags: 0x0
Entry point: 0x41fa60
Program header: 0x40..0x318
Section header: 0xdcce8..0xdd3e8

$ elb-cli show -t all /bin/sh
...
```


### Show dependencies

```sh
$ elb-cli deps -f list --hard-coded-search-dirs --names-only /bin/ls
libgcc_s.so.1
ld-linux-x86-64.so.2
libc.so.6
libcap.so.2.64

$ elb-cli deps -f tree --hard-coded-search-dirs --names-only /bin/ls
ls
 ├── libcap.so.2.64
 │   ├── libgcc_s.so.1
 │   │   ╰── libc.so.6
 │   │       ╰── ld-linux-x86-64.so.2
 │   ├── libc.so.6
 │   │   ╰── ld-linux-x86-64.so.2
 │   ╰── ld-linux-x86-64.so.2
 ├── libc.so.6
 │   ╰── ld-linux-x86-64.so.2
 ╰── ld-linux-x86-64.so.2
```


### Patch ELF

```sh
$ elb-cli patch \
    --set-interpreter /chroot/lib64/ld-linux-x86-64.so.2 \
    --set-dynamic RUNPATH=/chroot/lib64:/chroot/usr/lib64 \
    /chroot/bin/ls
```
