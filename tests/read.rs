use fs_err::File;
use std::env::split_paths;
use std::env::var_os;
use std::path::PathBuf;

use elfie::Elf;
use elfie::Error;
use walkdir::WalkDir;

#[test]
fn read_elf_files_from_file_system() {
    let mut paths: Vec<PathBuf> = Vec::new();
    paths.extend(DEFAULT_PATH.iter().map(Into::into));
    paths.extend(DEFAULT_LD_LIBRARY_PATH.iter().map(Into::into));
    for var_name in DEFAULT_ENV_VARS {
        append_paths_from_env(var_name, &mut paths);
    }
    paths.sort_unstable();
    paths.dedup();
    for dir in paths.iter() {
        if !dir.exists() || !dir.is_dir() {
            continue;
        }
        for entry in WalkDir::new(dir).into_iter() {
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(mut file) = File::open(path) else {
                continue;
            };
            eprintln!("Reading {:?}", path);
            let elf = match Elf::read_unchecked(&mut file, PAGE_SIZE) {
                Ok(elf) => elf,
                Err(Error::NotElf) => continue,
                Err(e) => {
                    panic!("Failed to parse {:?}: {e}", path);
                }
            };
            if let Err(e) = elf.validate() {
                panic!("Failed to validate {:?}: {e}", path);
            }
        }
    }
}

fn append_paths_from_env(var_name: &str, paths: &mut Vec<PathBuf>) {
    let Some(value) = var_os(var_name) else {
        return Default::default();
    };
    paths.extend(split_paths(&value).map(Into::into))
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
