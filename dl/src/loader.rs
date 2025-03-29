use std::collections::VecDeque;
use std::ffi::CString;
use std::ffi::OsStr;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use elfie::Class;
use elfie::DynamicTag;
use elfie::Elf;
use elfie::ElfPatcher;
use elfie::Machine;
use fs_err::File;
use glob::glob;
use log::log_enabled;
use log::trace;
use log::warn;
use log::Level::Trace;

use crate::Error;

pub struct DynamicLoader {
    pub system_search_paths: Vec<PathBuf>,
    page_size: u64,
}

impl DynamicLoader {
    pub fn from_rootfs_dir<P: AsRef<Path>>(rootfs_dir: P) -> Result<Self, Error> {
        Ok(Self {
            system_search_paths: get_search_dirs(rootfs_dir)?,
            page_size: 4096,
        })
    }

    pub fn set_page_size(&mut self, value: u64) {
        assert!(value.is_power_of_two());
        self.page_size = value;
    }

    pub fn resolve_dependencies<P: AsRef<Path>>(
        &self,
        file: P,
    ) -> Result<(Elf, Vec<PathBuf>), Error> {
        let mut file_names: Vec<CString> = Vec::new();
        let mut dependencies: Vec<PathBuf> = Vec::new();
        let dependent_file = file.as_ref();
        let mut file = File::open(dependent_file)?;
        let elf = Elf::read(&mut file, self.page_size)?;
        let mut patcher = ElfPatcher::new(elf, file);
        let dynstr_table = patcher.read_dynamic_string_table()?;
        let dynamic_table = patcher.read_dynamic_table()?;
        let interpreter = patcher
            .read_interpreter()?
            .map(|c_str| PathBuf::from(OsStr::from_bytes(c_str.to_bytes())));
        let mut search_paths = Vec::new();
        for key in [DynamicTag::RunPathOffset, DynamicTag::RpathOffset] {
            for dir in dynamic_table
                .iter()
                .filter_map(|(tag, value)| {
                    if *tag == key {
                        dynstr_table.get_string(*value as usize)
                    } else {
                        None
                    }
                })
                .flat_map(|rpath| std::env::split_paths(OsStr::from_bytes(rpath.to_bytes())))
            {
                let dir = interpolate(&dir, dependent_file, patcher.elf());
                if log_enabled!(Trace) {
                    let what = match key {
                        DynamicTag::RpathOffset => "rpath",
                        DynamicTag::RunPathOffset => "runpath",
                        _ => "library path",
                    };
                    trace!("Found {} {:?} in {:?}", what, dir, dependent_file);
                }
                search_paths.push(dir);
            }
        }
        if let Some(interpreter) = interpreter.as_ref() {
            if let Some(file_name) = interpreter.file_name() {
                trace!("Resolved {:?} as {:?}", file_name, interpreter);
                if !dependencies.contains(interpreter) {
                    let mut bytes = file_name.as_bytes().to_vec();
                    bytes.push(0_u8);
                    let c_string = CString::from_vec_with_nul(bytes).expect("Added NUL above");
                    file_names.push(c_string);
                    dependencies.push(interpreter.clone());
                }
            }
        }
        search_paths.extend(self.system_search_paths.clone());
        'outer: for (tag, value) in dynamic_table.iter() {
            if *tag != DynamicTag::Needed {
                continue;
            }
            let Some(dep_name) = dynstr_table.get_string(*value as usize) else {
                continue;
            };
            trace!("{:?} depends on {:?}", dependent_file, dep_name);
            for dir in search_paths.iter() {
                let path = dir.join(OsStr::from_bytes(dep_name.to_bytes()));
                let mut file = match File::open(&path) {
                    Ok(file) => file,
                    Err(ref e) if e.kind() == ErrorKind::NotFound => continue,
                    Err(e) => {
                        warn!("Failed to open {path:?}: {e}");
                        continue;
                    }
                };
                let dep = match Elf::read_unchecked(&mut file, self.page_size) {
                    Ok(dep) => dep,
                    Err(elfie::Error::NotElf) => continue,
                    Err(e) => return Err(e.into()),
                };
                let elf = patcher.elf();
                if dep.header.byte_order == elf.header.byte_order
                    && dep.header.class == elf.header.class
                    && dep.header.machine == elf.header.machine
                {
                    trace!("Resolved {:?} as {:?}", dep_name, path);
                    if !dependencies.contains(&path) {
                        dependencies.push(path.clone());
                    }
                    continue 'outer;
                }
            }
            trace!("Search paths {:#?}", search_paths);
            trace!("Resolved file names {:#?}", file_names);
            return Err(Error::FailedToResolve(
                dep_name.into(),
                dependent_file.to_path_buf(),
            ));
        }
        let (elf, _file) = patcher.into_inner();
        Ok((elf, dependencies))
    }
}

pub fn get_search_dirs<P: AsRef<Path>>(rootfs_dir: P) -> Result<Vec<PathBuf>, std::io::Error> {
    let rootfs_dir = rootfs_dir.as_ref();
    let mut paths = Vec::new();
    parse_ld_so_conf(rootfs_dir.join("etc/ld.so.conf"), rootfs_dir, &mut paths)?;
    if paths.is_empty() {
        paths.extend([
            rootfs_dir.join("lib"),
            rootfs_dir.join("usr/local/lib"),
            rootfs_dir.join("usr/lib"),
        ]);
    }
    if log_enabled!(Trace) {
        for path in paths.iter() {
            trace!("Found system library path {:?}", path);
        }
    }
    Ok(paths)
}

fn parse_ld_so_conf(
    path: PathBuf,
    rootfs_dir: &Path,
    paths: &mut Vec<PathBuf>,
) -> Result<(), std::io::Error> {
    let mut conf_files = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(path);
    while let Some(path) = queue.pop_front() {
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(ref e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => {
                warn!("Failed to open {path:?}: {e}");
                continue;
            }
        };
        conf_files.push(path);
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            let line = match line.find('#') {
                Some(i) => &line[..i],
                None => &line[..],
            }
            .trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with("include") {
                let Some(i) = line.find(char::is_whitespace) else {
                    // Malformed "include" directive.
                    continue;
                };
                let pattern = &line[i + 1..];
                let Ok(more_paths) = glob(pattern) else {
                    // Unparsable glob pattern.
                    continue;
                };
                for path in more_paths {
                    let Ok(path) = path else {
                        continue;
                    };
                    let Ok(path) = path.strip_prefix("/") else {
                        continue;
                    };
                    let path = rootfs_dir.join(path);
                    if !conf_files.contains(&path) {
                        queue.push_back(path);
                    }
                }
            }
            if let Some(path) = line.strip_prefix("/") {
                paths.push(rootfs_dir.join(path));
            }
        }
    }
    Ok(())
}

fn interpolate(dir: &Path, file: &Path, elf: &Elf) -> PathBuf {
    use Component::*;
    let mut interpolated = PathBuf::new();
    for comp in dir.components() {
        match comp {
            Normal(comp) if comp == "$ORIGIN" || comp == "${ORIGIN}" => {
                if let Some(parent) = file.parent() {
                    interpolated.push(parent);
                } else {
                    interpolated.push(comp);
                }
            }
            Normal(comp) if comp == "$LIB" || comp == "${LIB}" => {
                let lib = match elf.header.class {
                    Class::Elf32 => "lib",
                    Class::Elf64 => "lib64",
                };
                interpolated.push(lib);
            }
            // TODO more platforms
            Normal(comp) if comp == "$PLATFORM" || comp == "${PLATFORM}" => {
                let platform = match elf.header.machine {
                    Machine::X86_64 => "x86_64",
                    _ => {
                        warn!(
                            "Failed to interpolate $PLATFORM, machine is {:?} ({})",
                            elf.header.machine,
                            elf.header.machine.as_u16()
                        );
                        interpolated.push(comp);
                        continue;
                    }
                };
                interpolated.push(platform);
            }
            comp => interpolated.push(comp),
        }
    }
    interpolated
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs_err::OpenOptions;
    use std::collections::HashSet;
    use std::env::split_paths;
    use std::env::var_os;
    use std::ffi::CStr;
    use std::ffi::OsString;
    use std::fs::Permissions;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::process::Stdio;
    use tempfile::TempDir;
    use walkdir::WalkDir;

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
        let mut loader = DynamicLoader::from_rootfs_dir("/").unwrap();
        // TODO
        loader
            .system_search_paths
            .push("/gnu/store/hw6g2kjayxnqi8rwpnmpraalxi0djkxc-glibc-2.39/lib".into());
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
                            let Some(_) =
                                data.windows(bytes.len()).position(|window| window == bytes)
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
                            let tmpdir = TempDir::with_prefix("elfie-test-").unwrap();
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
                                let (elf, _deps) = loader.resolve_dependencies(&dep_file).unwrap();
                                let file = OpenOptions::new()
                                    .read(true)
                                    .write(true)
                                    .open(&new_file)
                                    .unwrap();
                                let mut patcher = ElfPatcher::new(elf, file);
                                let dynamic_table = patcher.read_dynamic_table().unwrap();
                                if !dynamic_table
                                    .iter()
                                    .any(|(tag, _)| *tag == DynamicTag::Needed)
                                {
                                    // Statically linked.
                                    continue;
                                }
                                patcher.remove_interpreter().unwrap();
                                /*
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
                                elf.set_dynamic_c_str(
                                    &mut file,
                                    DynamicTag::RunPathOffset,
                                    &run_path,
                                )
                                .unwrap();
                                */
                                patcher.finish().unwrap();
                            }
                            let new_path = workdir.join(path.file_name().unwrap());
                            fs_err::copy(&path, &new_path).unwrap();
                            fs_err::set_permissions(&new_path, Permissions::from_mode(0o755))
                                .unwrap();
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
                            patcher.remove_dynamic(DynamicTag::RunPathOffset).unwrap();
                            patcher
                                .set_dynamic_c_str(DynamicTag::RpathOffset, &run_path)
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
                    Err(Error::Elf(elfie::Error::NotElf)) => continue,
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
}
