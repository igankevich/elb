use std::ffi::CString;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use crate::fs::File;
use elb::Class;
use elb::DynamicTag;
use elb::Elf;
use elb::Machine;
use log::log_enabled;
use log::trace;
use log::warn;
use log::Level::Trace;

use crate::Error;

/// Dynamic loader options.
pub struct LoaderOptions {
    search_dirs: Vec<PathBuf>,
    lib: Option<OsString>,
    platform: Option<OsString>,
    page_size: u64,
}

impl LoaderOptions {
    /// Default options.
    pub fn new() -> Self {
        Self {
            search_dirs: Default::default(),
            lib: None,
            platform: None,
            page_size: 4096,
        }
    }

    /// Directories where to look for libraries.
    ///
    /// See [`glibc`](crate::glibc) and [`musl`](crate::musl) modules.
    pub fn search_dirs(mut self, search_dirs: Vec<PathBuf>) -> Self {
        self.search_dirs = search_dirs;
        self
    }

    /// Set page size.
    ///
    /// Panics if the size is not a power of two.
    pub fn page_size(mut self, page_size: u64) -> Self {
        assert!(page_size.is_power_of_two());
        self.page_size = page_size;
        self
    }

    /// Set library directory name.
    ///
    /// This value is used to substitute `$LIB` variable in `RPATH` and `RUNPATH`.
    ///
    /// When not set `lib` is used for 32-bit arhitectures and `lib64` is used for 64-bit
    /// architectures.
    pub fn lib(mut self, lib: Option<OsString>) -> Self {
        self.lib = lib;
        self
    }

    /// Set platform directory name.
    ///
    /// This value is used to substitute `$PLATFORM` variable in `RPATH` and `RUNPATH`.
    ///
    /// When not set the platform is interpolated based on [`Machine`](elb::Machine)
    /// (best-effort).
    pub fn platform(mut self, platform: Option<OsString>) -> Self {
        self.platform = platform;
        self
    }

    /// Create new dynamic loader using the current options.
    pub fn new_loader(self) -> DynamicLoader {
        DynamicLoader {
            search_dirs: self.search_dirs,
            lib: self.lib,
            platform: self.platform,
            page_size: self.page_size,
        }
    }
}

impl Default for LoaderOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Dynamic loader.
///
/// Resolved ELF dependencies without loading and executing the files.
pub struct DynamicLoader {
    search_dirs: Vec<PathBuf>,
    lib: Option<OsString>,
    platform: Option<OsString>,
    page_size: u64,
}

impl DynamicLoader {
    /// Get default loader options.
    pub fn options() -> LoaderOptions {
        LoaderOptions::new()
    }

    /// Find immediate dependencies of the ELF `file`.
    ///
    /// To find all dependencies, recursively pass each returned path to this method again.
    pub fn resolve_dependencies<P: AsRef<Path>>(
        &self,
        file: P,
    ) -> Result<(Elf, Vec<PathBuf>), Error> {
        let mut file_names: Vec<CString> = Vec::new();
        let mut dependencies: Vec<PathBuf> = Vec::new();
        let dependent_file = file.as_ref();
        let mut file = File::open(dependent_file)?;
        let elf = Elf::read(&mut file, self.page_size)?;
        let names = elf.read_section_names(&mut file)?.unwrap_or_default();
        let dynstr_table = elf
            .read_dynamic_string_table(&mut file)?
            .unwrap_or_default();
        let Some(dynamic_table) = elf.read_dynamic_table(&mut file)? else {
            return Ok((elf, Default::default()));
        };
        let interpreter = elf
            .read_interpreter(&names, &mut file)?
            .map(|c_str| PathBuf::from(OsStr::from_bytes(c_str.to_bytes())));
        let mut search_dirs = Vec::new();
        for key in [DynamicTag::Runpath, DynamicTag::Rpath] {
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
                let dir = interpolate(
                    &dir,
                    dependent_file,
                    &elf,
                    self.lib.as_deref(),
                    self.platform.as_deref(),
                );
                if log_enabled!(Trace) {
                    let what = match key {
                        DynamicTag::Rpath => "rpath",
                        DynamicTag::Runpath => "runpath",
                        _ => "library path",
                    };
                    trace!("Found {} {:?} in {:?}", what, dir, dependent_file);
                }
                search_dirs.push(dir);
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
        search_dirs.extend(self.search_dirs.clone());
        'outer: for (tag, value) in dynamic_table.iter() {
            if *tag != DynamicTag::Needed {
                continue;
            }
            let Some(dep_name) = dynstr_table.get_string(*value as usize) else {
                continue;
            };
            trace!("{:?} depends on {:?}", dependent_file, dep_name);
            for dir in search_dirs.iter() {
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
                    Err(elb::Error::NotElf) => continue,
                    Err(e) => return Err(e.into()),
                };
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
            return Err(Error::FailedToResolve(
                dep_name.into(),
                dependent_file.to_path_buf(),
            ));
        }
        Ok((elf, dependencies))
    }
}

fn interpolate(
    dir: &Path,
    file: &Path,
    elf: &Elf,
    lib: Option<&OsStr>,
    platform: Option<&OsStr>,
) -> PathBuf {
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
                let lib = match lib {
                    Some(lib) => lib,
                    None => match elf.header.class {
                        Class::Elf32 => OsStr::new("lib"),
                        Class::Elf64 => OsStr::new("lib64"),
                    },
                };
                interpolated.push(lib);
            }
            Normal(comp) if comp == "$PLATFORM" || comp == "${PLATFORM}" => {
                if let Some(platform) = platform {
                    interpolated.push(platform);
                } else {
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
            }
            comp => interpolated.push(comp),
        }
    }
    interpolated
}
