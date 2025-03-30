#![allow(clippy::unwrap_used)]
#![allow(missing_docs)]

use std::collections::HashSet;
use std::collections::VecDeque;
use std::env::split_paths;
use std::env::var_os;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs::Permissions;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use elb::Elf;
use elb::ElfPatcher;
use fs_err::File;
use fs_err::OpenOptions;
use tempfile::TempDir;
use walkdir::WalkDir;

use elb::DynamicTag;
use elb_dl::glibc;
use elb_dl::DynamicLoader;
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
    let loader = DynamicLoader::options()
        .page_size(4096)
        .search_dirs({
            let mut dirs = Vec::new();
            dirs.extend(glibc::get_hard_coded_search_dirs(None).unwrap());
            dirs.extend(glibc::get_search_dirs("/").unwrap());
            dirs
        })
        .new_loader();
    let mut visited = HashSet::new();
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
            //eprintln!("Reading {:?}", path);
            match loader.resolve_dependencies(&path) {
                Ok((elf, dependencies)) => {
                    let file = File::open(&path).unwrap();
                    let mut patcher = ElfPatcher::new(elf, file);
                    let Ok(Some(_)) = patcher.read_interpreter() else {
                        continue;
                    };
                    let Ok(Some(data)) = patcher.read_section(c".rodata") else {
                        continue;
                    };
                    let mut working_arg = None;
                    for arg in [c"--version", c"--help"] {
                        let bytes = arg.to_bytes_with_nul();
                        // remove dashes
                        let bytes = &bytes[2..];
                        let Some(_) = data.windows(bytes.len()).position(|window| window == bytes)
                        else {
                            continue;
                        };
                        eprintln!("{path:?}: Found {arg:?}");
                        working_arg = Some(arg);
                        break;
                    }
                    let mut copied_files_hashes = HashSet::new();
                    if let Some(arg) = working_arg {
                        let arg = OsStr::from_bytes(arg.to_bytes());
                        let expected_result =
                            Command::new(&path).arg(arg).stdin(Stdio::null()).status();
                        if expected_result.is_err() {
                            continue;
                        }
                        eprintln!("Result {:?}", expected_result);
                        let tmpdir = TempDir::with_prefix("elb-test-").unwrap();
                        let workdir = tmpdir.path();
                        fs_err::create_dir_all(workdir).unwrap();
                        let mut queue = VecDeque::new();
                        queue.extend(dependencies.iter().cloned());
                        while let Some(dep_file) = queue.pop_front() {
                            eprintln!("Dependency {:?}", dep_file);
                            let file_hash = hash_file(&dep_file);
                            if !copied_files_hashes.insert(file_hash.clone()) {
                                continue;
                            }
                            let file_name = dep_file.file_name().unwrap();
                            let new_dir = workdir.join(&file_hash);
                            fs_err::create_dir_all(&new_dir).unwrap();
                            let new_file = new_dir.join(file_name);
                            fs_err::copy(&dep_file, &new_file).unwrap();
                            fs_err::set_permissions(&new_file, Permissions::from_mode(0o755))
                                .unwrap();
                            let (elf, deps) = loader.resolve_dependencies(&dep_file).unwrap();
                            let file = OpenOptions::new()
                                .read(true)
                                .write(true)
                                .open(&new_file)
                                .unwrap();
                            let mut patcher = ElfPatcher::new(elf, file);
                            let dynamic_table =
                                patcher.read_dynamic_table().unwrap().unwrap_or_default();
                            if !dynamic_table
                                .iter()
                                .any(|(tag, _)| *tag == DynamicTag::Needed)
                            {
                                // Statically linked.
                                continue;
                            }
                            patcher.remove_interpreter().unwrap();
                            let run_path = {
                                let mut bytes = Vec::new();
                                for dep in deps.into_iter() {
                                    if !bytes.is_empty() {
                                        bytes.push(b':');
                                    }
                                    let file_hash = hash_file(&dep);
                                    bytes.extend_from_slice(workdir.as_os_str().as_bytes());
                                    bytes.push(b'/');
                                    bytes.extend_from_slice(file_hash.as_bytes());
                                    queue.push_back(dep);
                                }
                                bytes.push(0_u8);
                                CString::from_vec_with_nul(bytes).unwrap()
                            };
                            patcher
                                .set_library_search_path(DynamicTag::Runpath, run_path.as_c_str())
                                .unwrap();
                            patcher.finish().unwrap();
                        }
                        let new_path = workdir.join(path.file_name().unwrap());
                        fs_err::copy(&path, &new_path).unwrap();
                        fs_err::set_permissions(&new_path, Permissions::from_mode(0o755)).unwrap();
                        let mut file = OpenOptions::new()
                            .read(true)
                            .write(true)
                            .open(&new_path)
                            .unwrap();
                        // TODO
                        let elf = Elf::read(&mut file, 4096).unwrap();
                        let mut patcher = ElfPatcher::new(elf, file);
                        let interpreter = patcher.read_interpreter().unwrap().unwrap();
                        let interpreter: PathBuf =
                            OsString::from_vec(interpreter.into_bytes()).into();
                        let interpreter_hash = hash_file(&interpreter);
                        let new_interpreter = workdir
                            .join(&interpreter_hash)
                            .join(interpreter.file_name().unwrap());
                        eprintln!("New interpreter {:?}", new_interpreter);
                        let mut new_interpreter = new_interpreter.into_os_string();
                        new_interpreter.push("\0");
                        patcher
                            .set_interpreter(
                                CStr::from_bytes_with_nul(new_interpreter.as_bytes()).unwrap(),
                            )
                            .unwrap();
                        //let mut file = patcher.finish().unwrap();
                        //file.seek(SeekFrom::Start(0)).unwrap();
                        //// TODO
                        //let elf = Elf::read(&mut file, 4096).unwrap();
                        //let mut patcher = ElfPatcher::new(elf, file);
                        let run_path = {
                            let mut bytes = Vec::new();
                            for dep in dependencies.into_iter() {
                                if !bytes.is_empty() {
                                    bytes.push(b':');
                                }
                                let file_hash = hash_file(&dep);
                                bytes.extend_from_slice(workdir.as_os_str().as_bytes());
                                bytes.push(b'/');
                                bytes.extend_from_slice(file_hash.as_bytes());
                            }
                            bytes.push(0_u8);
                            CString::from_vec_with_nul(bytes).unwrap()
                        };
                        patcher
                            .set_library_search_path(DynamicTag::Runpath, run_path.as_c_str())
                            .unwrap();
                        patcher.finish().unwrap();
                        let actual_result = Command::new(&new_path)
                            .arg(arg)
                            .stdin(Stdio::null())
                            .status();
                        let expected = expected_result.unwrap();
                        let actual = actual_result.unwrap();
                        if expected != actual {
                            let workdir = workdir.to_path_buf();
                            std::mem::forget(tmpdir);

                            panic!(
                                    "Expected {expected:?}, actual {actual:?}, command {:?} {:?}, files {:?}",
                                    path,
                                    arg,
                                    workdir
                                );
                        }
                        eprintln!("SUCCESS {:?}", path);
                    }
                }
                Err(Error::Elf(elb::Error::NotElf)) => continue,
                Err(e) => {
                    panic!("Failed to process {:?}: {e}", path);
                }
            }
        }
    }
}

fn hash_file<P: AsRef<Path>>(path: P) -> String {
    use base32::Alphabet;
    use sha2::Digest;
    let mut file = File::open(path.as_ref()).unwrap();
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).unwrap();
    let hash = hasher.finalize();
    base32::encode(Alphabet::Crockford, &hash[..]).to_lowercase()
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
];
