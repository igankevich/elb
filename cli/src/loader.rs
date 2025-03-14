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
use fs_err::File;
use glob::glob;
use log::log_enabled;
use log::trace;
use log::warn;
use log::Level::Trace;

#[derive(thiserror::Error, Debug)]
pub enum LoaderError {
    #[error("ELF error: {0}")]
    Elf(#[from] elfie::Error),
    #[error("Failed to resolve dependency {0:?} of {1:?}")]
    FailedToResolve(CString, PathBuf),
    #[error("Input/output error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct DynamicLoader {
    // TODO set?
    system_search_paths: Vec<PathBuf>,
}

impl DynamicLoader {
    pub fn from_rootfs_dir<P: AsRef<Path>>(rootfs_dir: P) -> Result<Self, LoaderError> {
        let system_search_paths = get_library_search_paths(rootfs_dir)?;
        Ok(Self::from_system_search_paths(system_search_paths))
    }

    pub fn from_system_search_paths(system_search_paths: Vec<PathBuf>) -> Self {
        Self {
            system_search_paths,
        }
    }

    #[allow(unused)]
    pub fn add_system_search_path<P: Into<PathBuf>>(&mut self, path: P) {
        self.system_search_paths.push(path.into());
    }

    pub fn resolve_dependencies<P: AsRef<Path>>(
        &self,
        file: P,
    ) -> Result<Vec<PathBuf>, LoaderError> {
        let mut file_names: Vec<CString> = Vec::new();
        let mut dependencies: Vec<PathBuf> = Vec::new();
        let mut queue = VecDeque::new();
        let file = fs_err::canonicalize(file.as_ref())?;
        queue.push_back(file);
        while let Some(dependent_file) = queue.pop_front() {
            let mut file = File::open(&dependent_file)?;
            let elf = Elf::read(&mut file)?;
            let dynstr_table = elf.read_dynamic_string_table(&mut file)?;
            let dynamic_table = elf.read_dynamic_table(&mut file)?;
            let interpreter = elf
                .read_interpreter(&mut file)?
                .map(|c_str| PathBuf::from(OsStr::from_bytes(c_str.to_bytes())));
            let mut search_paths = Vec::new();
            for key in [DynamicTag::RpathOffset, DynamicTag::RunPathOffset] {
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
                    let dir = interpolate(&dir, &dependent_file, &elf);
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
                    let dep = match Elf::read_unchecked(&mut file) {
                        Ok(dep) => dep,
                        Err(elfie::Error::NotElf) => continue,
                        Err(e) => return Err(e.into()),
                    };
                    if dep.header.byte_order == elf.header.byte_order
                        && dep.header.class == elf.header.class
                        && dep.header.machine == elf.header.machine
                    {
                        trace!("Resolved {:?} as {:?}", dep_name, path);
                        if !dependencies.contains(&path) {
                            dependencies.push(path.clone());
                            queue.push_back(path);
                        }
                        continue 'outer;
                    }
                }
                trace!("Search paths {:#?}", search_paths);
                trace!("Resolved file names {:#?}", file_names);
                return Err(LoaderError::FailedToResolve(
                    dep_name.into(),
                    dependent_file,
                ));
            }
        }
        Ok(dependencies)
    }
}

pub fn get_library_search_paths<P: AsRef<Path>>(
    rootfs_dir: P,
) -> Result<Vec<PathBuf>, std::io::Error> {
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
            Normal(comp) if comp == "$PLATFORM" || comp == "${PLATFOMR}" => {
                let platform = match elf.header.machine {
                    0x3e => "x86_64",
                    _ => {
                        warn!(
                            "Failed to interpolate $PLATFORM, machine is {:#x}",
                            elf.header.machine
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
    use crate::Logger;
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
        Logger::init(true).unwrap();
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
        loader.add_system_search_path("/gnu/store/hw6g2kjayxnqi8rwpnmpraalxi0djkxc-glibc-2.39/lib");
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
                    continue;
                }
                let Ok(path) = path.canonicalize() else {
                    continue;
                };
                if !visited.insert(path.clone()) {
                    continue;
                }
                //eprintln!("Reading {:?}", path);
                match loader.resolve_dependencies(&path) {
                    Ok(dependencies) => {
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
                        let file_name = file_name.as_bytes();
                        if file_name.starts_with(b"lib")
                            && file_name.windows(3).any(|window| window == b".so")
                        {
                            continue;
                        }
                        let mut file = File::open(&path).unwrap();
                        let elf = Elf::read(&mut file).unwrap();
                        let Ok(Some(_)) = elf.read_interpreter(&mut file) else {
                            continue;
                        };
                        let Ok(Some(data)) = elf.read_section(c".rodata", &mut file) else {
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
                            for file in dependencies.iter() {
                                eprintln!("Dependency {:?}", file);
                                let file_name = file.file_name().unwrap();
                                let new_file = workdir.join(file_name);
                                fs_err::copy(file, &new_file).unwrap();
                                fs_err::set_permissions(&new_file, Permissions::from_mode(0o755))
                                    .unwrap();
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
                            let mut elf = Elf::read(&mut file).unwrap();
                            let interpreter = elf.read_interpreter(&mut file).unwrap().unwrap();
                            let interpreter: PathBuf =
                                OsString::from_vec(interpreter.into_bytes()).into();
                            let new_interpreter = workdir.join(interpreter.file_name().unwrap());
                            eprintln!("New interpreter {:?}", new_interpreter);
                            let mut new_interpreter = new_interpreter.into_os_string();
                            new_interpreter.push("\0");
                            elf.set_interpreter(
                                &mut file,
                                CStr::from_bytes_with_nul(new_interpreter.as_bytes()).unwrap(),
                            )
                            .unwrap();
                            let run_path = {
                                let mut bytes = workdir.as_os_str().as_bytes().to_vec();
                                bytes.push(0_u8);
                                CString::from_vec_with_nul(bytes).unwrap()
                            };
                            elf.set_dynamic_c_str(&mut file, DynamicTag::RunPathOffset, &run_path)
                                .unwrap();
                            elf.write(&mut file).unwrap();
                            drop(file);
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
                        }
                    }
                    Err(LoaderError::Elf(elfie::Error::NotElf)) => continue,
                    Err(e) => {
                        panic!("Failed to process {:?}: {e}", path);
                    }
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

    const NOT_WORKING: &[&str] = &[
        // qt/plugins needs to be copied to RUNPATH
        "scribus",
        // SIGSEGV, garbled `ldd` output
        "convco",
        "cargo-sqlx",
        "mdbook-linkcheck",
        "cargo-msrv",
        "sqlx",
        "cargo-about",
        "cargo-deny",
        "darktable-rs-identify",
        "chromium",
        // no --version arg
        "FBReader",
        "chromedriver",
    ];
}
