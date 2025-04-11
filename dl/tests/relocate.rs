#![allow(clippy::unwrap_used)]
#![allow(missing_docs)]

use std::collections::HashSet;
use std::env::split_paths;
use std::env::var_os;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use elb::Elf;
use fs_err::read_dir;
use fs_err::File;
use tempfile::TempDir;

use elb_dl::glibc;
use elb_dl::DynamicLoader;
use elb_dl::ElfRelocator;
use elb_dl::Error;

#[test]
fn loader_resolves_system_files() {
    let _ = env_logger::try_init();
    let mut paths: Vec<PathBuf> = Vec::new();
    paths.extend(DEFAULT_PATH.iter().map(Into::into));
    paths.extend(DEFAULT_LD_LIBRARY_PATH.iter().map(Into::into));
    for var_name in DEFAULT_ENV_VARS {
        append_paths_from_env(var_name, &mut paths);
    }
    paths.sort_unstable();
    paths.dedup();
    eprintln!("ELF search directories: {:#?}", paths);
    let page_size = page_size::get() as u64;
    let loader = DynamicLoader::options()
        .page_size(page_size)
        .search_dirs({
            let mut dirs = Vec::new();
            dirs.extend(glibc::get_hard_coded_search_dirs(None).unwrap());
            dirs.extend(glibc::get_search_dirs("/").unwrap());
            eprintln!("Library search directories: {:#?}", dirs);
            dirs
        })
        .new_loader();
    let relocator = ElfRelocator::new(loader);
    let mut visited = HashSet::new();
    let mut num_checked: usize = 0;
    for path in paths.iter() {
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
                // Not a regular file or a symlink to a regular file.
                continue;
            }
            let Ok(path) = path.canonicalize() else {
                continue;
            };
            if !visited.insert(path.clone()) {
                // Already visited.
                continue;
            }
            let metadata = fs_err::metadata(&path).unwrap();
            if metadata.mode() & 0o7000 != 0 {
                // Ignore setuid files.
                continue;
            }
            // TODO check arch
            let Some(file_name) = path.file_name() else {
                continue;
            };
            // TODO
            //if file_name.to_str().unwrap_or_default().contains("systemd") {
            //    continue;
            //}
            if NOT_WORKING.contains(&file_name.to_str().unwrap_or_default()) {
                // Known to not work.
                continue;
            }
            if file_name.as_bytes().starts_with(b"lib")
                && file_name
                    .as_bytes()
                    .windows(3)
                    .any(|window| window == b".so")
            {
                continue;
            }
            let tmpdir = TempDir::with_prefix("elb-test-").unwrap();
            let workdir = tmpdir.path();
            let new_path = match relocator.relocate(&path, workdir) {
                Ok(new_path) => new_path,
                Err(Error::Elf(elb::Error::NotElf)) => continue,
                Err(e) => {
                    panic!("Failed to process {:?}: {e}", path);
                }
            };
            // Check that we can execute this binary with `--help` or `--version` argument.
            let mut file = File::open(&path).unwrap();
            let elf = Elf::read(&mut file, page_size).unwrap();
            let Some(_) = elf.read_interpreter(&mut file).unwrap() else {
                continue;
            };
            let Some(names) = elf.read_section_names(&mut file).unwrap() else {
                continue;
            };
            let Ok(Some(data)) = elf.read_section(c".rodata", &names, &mut file) else {
                continue;
            };
            let mut working_arg = None;
            for arg in [c"--version", c"--help"] {
                let bytes = arg.to_bytes_with_nul();
                // remove dashes
                let bytes = &bytes[2..];
                let Some(_) = data.windows(bytes.len()).position(|window| window == bytes) else {
                    continue;
                };
                eprintln!("{path:?}: Found {arg:?}");
                working_arg = Some(arg);
                break;
            }
            let Some(working_arg) = working_arg else {
                continue;
            };
            let arg = OsStr::from_bytes(working_arg.to_bytes());
            // Execute the original binary.
            let expected_result = Command::new(&path)
                .arg(arg)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .status();
            if expected_result.is_err() {
                continue;
            }
            eprintln!("Result {:?}", expected_result);
            // Now execute the relocated binary.
            let actual_result = Command::new(&new_path)
                .arg(arg)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .status();
            let expected = expected_result.unwrap();
            let actual = actual_result.unwrap();
            if expected != actual {
                let workdir = workdir.to_path_buf();
                std::mem::forget(tmpdir);
                panic!(
                    "Expected {expected:?}, actual {actual:?}, command {:?} {:?}, files {:?}",
                    path, arg, workdir
                );
            }
            eprintln!("SUCCESS {:?}", path);
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

const NOT_WORKING: &[&str] = &[
    // qt/plugins needs to be copied to RUNPATH
    "scribus",
    // connect fails after a few retries
    "jack_transport",
    // segmentation fault
    "cargo-deny",
    // no --version arg
    "FBReader",
    // Hangs on Debian, returns 1 on Ubuntu:
    // mtr-packet: Failure to open IPv4 sockets: Permission denied
    // mtr-packet: Failure to open IPv6 sockets: Permission denied
    "mtr-packet",
    // A JSON parsing exception occurred in [/tmp/elb-test-2rconN/bicep.runtimeconfig.json], offset
    // 0 (line 1, column 1): Invalid value.
    "bicep",
];
