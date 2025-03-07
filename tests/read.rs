#![allow(missing_docs)]

use fs_err::read_dir;
use fs_err::File;
use std::env::split_paths;
use std::env::var_os;
use std::path::PathBuf;

use elb::Elf;
use elb::Error;

#[test]
fn read_elf_files_from_file_system() {
    let mut dirs: Vec<PathBuf> = Vec::new();
    dirs.extend(DEFAULT_PATH.iter().map(Into::into));
    dirs.extend(DEFAULT_LD_LIBRARY_PATH.iter().map(Into::into));
    for var_name in DEFAULT_ENV_VARS {
        append_paths_from_env(var_name, &mut dirs);
    }
    dirs.sort_unstable();
    dirs.dedup();
    eprintln!("ELF search directories: {:#?}", dirs);
    let mut num_checked: usize = 0;
    for path in dirs.iter() {
        eprintln!("Entering {:?}", path);
        if !path.exists() || !path.is_dir() {
            continue;
        }
        let Ok(dir) = read_dir(path) else {
            eprintln!("Failed to open directory {:?}", path);
            continue;
        };
        for entry in dir {
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(mut file) = File::open(&path) else {
                eprintln!("Failed to open file {:?}", path);
                continue;
            };
            let elf = match Elf::read_unchecked(&mut file, PAGE_SIZE) {
                Ok(elf) => elf,
                Err(Error::NotElf) => continue,
                Err(e) => {
                    panic!("Failed to parse {:?}: {e}", path);
                }
            };
            if let Err(e) = elf.check() {
                panic!("Failed to validate {:?}: {e}", path);
            }
            num_checked += 1;
        }
    }
    eprintln!("Checked {} file(s)", num_checked);
}

fn append_paths_from_env(var_name: &str, paths: &mut Vec<PathBuf>) {
    let Some(value) = var_os(var_name) else {
        return Default::default();
    };
    paths.extend(split_paths(&value))
}

/// Environment variables known to hold paths to ELF files.
const DEFAULT_ENV_VARS: [&str; 3] = ["LD_LIBRARY_PATH", "LIBRARY_PATH", "PATH"];

const DEFAULT_PATH: [&str; 6] = [
    "/bin",
    "/sbin",
    "/usr/bin",
    "/usr/local/bin",
    "/usr/local/sbin",
    "/usr/sbin",
];

const DEFAULT_LD_LIBRARY_PATH: [&str; 6] = [
    "/lib",
    "/lib64",
    "/usr/lib",
    "/usr/lib64",
    "/usr/local/lib",
    "/usr/local/lib64",
];

const PAGE_SIZE: u64 = 4096;
