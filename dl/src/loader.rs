use std::borrow::Borrow;
use std::env::split_paths;
use std::ffi::CStr;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::iter::IntoIterator;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use crate::fs::File;
use elb::Class;
use elb::DynamicTag;
use elb::Elf;
use elb::Machine;
use log::trace;
use log::warn;

use crate::Error;

/// Dependency table.
///
/// Acts as a dependency resolution cache as well.
#[derive(Debug)]
pub struct DependencyTree {
    dependencies: Vec<(PathBuf, Vec<PathBuf>)>,
}

impl DependencyTree {
    /// Create empty dependency tree.
    pub const fn new() -> Self {
        Self {
            dependencies: Vec::new(),
        }
    }

    /// Check if the tree contains the dependent specified by its canonical path.
    pub fn contains<P>(&self, canonical_path: &P) -> bool
    where
        PathBuf: Borrow<P>,
        P: Ord + ?Sized,
    {
        self.dependencies
            .binary_search_by(|(dependent, _)| dependent.borrow().cmp(canonical_path))
            .is_ok()
    }

    /// Get dependencies by canonical path of the dependent.
    pub fn get<P>(&self, canonical_path: &P) -> Option<&[PathBuf]>
    where
        PathBuf: Borrow<P>,
        P: Ord + ?Sized,
    {
        self.dependencies
            .binary_search_by(|(dependent, _)| dependent.borrow().cmp(canonical_path))
            .ok()
            .map(|i| self.dependencies[i].1.as_slice())
    }

    /// Insert new dependent and its dependencies.
    ///
    /// Returns the previous value if any.
    pub fn insert(
        &mut self,
        dependent: PathBuf,
        dependencies: Vec<PathBuf>,
    ) -> Option<Vec<PathBuf>> {
        match self
            .dependencies
            .binary_search_by(|(x, _)| x.cmp(&dependent))
        {
            Ok(i) => Some(std::mem::replace(&mut self.dependencies[i].1, dependencies)),
            Err(i) => {
                self.dependencies.insert(i, (dependent, dependencies));
                None
            }
        }
    }

    /// Remove the dependent and its dependencies from the tree.
    pub fn remove<P>(&mut self, canonical_path: &P) -> Option<Vec<PathBuf>>
    where
        PathBuf: Borrow<P>,
        P: Ord + ?Sized,
    {
        self.dependencies
            .binary_search_by(|(dependent, _)| dependent.borrow().cmp(canonical_path))
            .ok()
            .map(|i| self.dependencies.remove(i).1)
    }

    /// Get the number of dependents in the tree.
    pub fn len(&self) -> usize {
        self.dependencies.len()
    }

    /// Returns `true` if the tree doesn't have any dependents.
    pub fn is_empty(&self) -> bool {
        self.dependencies.is_empty()
    }
}

impl Default for DependencyTree {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for DependencyTree {
    type Item = (PathBuf, Vec<PathBuf>);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.dependencies.into_iter()
    }
}

/// Dynamic linker implementation that we're emulating.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Libc {
    /// GNU libc.
    #[default]
    Glibc,
    /// Musl libc.
    Musl,
}

/// Dynamic loader options.
pub struct LoaderOptions {
    search_dirs: Vec<PathBuf>,
    search_dirs_override: Vec<PathBuf>,
    lib: Option<OsString>,
    platform: Option<OsString>,
    page_size: u64,
    libc: Libc,
}

impl LoaderOptions {
    /// Default options.
    pub fn new() -> Self {
        Self {
            search_dirs: Default::default(),
            search_dirs_override: Default::default(),
            lib: None,
            platform: None,
            page_size: 4096,
            libc: Default::default(),
        }
    }

    /// Glibc-specific options.
    #[cfg(feature = "glibc")]
    pub fn glibc<P: AsRef<Path>>(rootfs_dir: P) -> Result<Self, std::io::Error> {
        Ok(Self {
            search_dirs: crate::glibc::get_search_dirs(rootfs_dir)?,
            search_dirs_override: get_search_dirs_from_env(),
            libc: Libc::Glibc,
            ..Default::default()
        })
    }

    /// Musl-specific options.
    #[cfg(feature = "musl")]
    pub fn musl<P: AsRef<Path>>(rootfs_dir: P, arch: &str) -> Result<Self, std::io::Error> {
        Ok(Self {
            search_dirs: crate::musl::get_search_dirs(rootfs_dir, arch)?,
            search_dirs_override: get_search_dirs_from_env(),
            libc: Libc::Musl,
            ..Default::default()
        })
    }

    /// Dynamic linker implementation that we're emulating.
    ///
    /// Affects library search order only.
    ///
    /// To also set library search directories, use [`glibc`](Self::glibc) and [`musl`](Self::musl)
    /// constructors.
    pub fn libc(mut self, libc: Libc) -> Self {
        self.libc = libc;
        self
    }

    /// Directories where to look for libraries *after* searching in the `RUNPATH` or in the
    /// `RPATH`.
    ///
    /// Use the following functions to initialize this field.
    /// - Glibc: [`glibc::get_search_dirs`](crate::glibc::get_search_dirs).
    /// - Musl: [`musl::get_search_dirs`](crate::musl::get_search_dirs).
    pub fn search_dirs(mut self, search_dirs: Vec<PathBuf>) -> Self {
        self.search_dirs = search_dirs;
        self
    }

    /// Directories where to look for libraries *before* searching in the `RUNPATH`.
    ///
    /// This list doesn't affect `RPATH`-based lookup.
    ///
    /// Use [`get_search_dirs_from_env`](crate::get_search_dirs_from_env) to initialize this field.
    pub fn search_dirs_override(mut self, search_dirs: Vec<PathBuf>) -> Self {
        self.search_dirs_override = search_dirs;
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
            search_dirs_override: self.search_dirs_override,
            lib: self.lib,
            platform: self.platform,
            page_size: self.page_size,
            libc: self.libc,
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
    search_dirs_override: Vec<PathBuf>,
    lib: Option<OsString>,
    platform: Option<OsString>,
    page_size: u64,
    libc: Libc,
}

impl DynamicLoader {
    /// Get default loader options.
    pub fn options() -> LoaderOptions {
        LoaderOptions::new()
    }

    /// Find immediate dependencies of the ELF `file`.
    ///
    /// To find all dependencies, recursively pass each returned path to this method again.
    pub fn resolve_dependencies<P: Into<PathBuf>>(
        &self,
        file: P,
        tree: &mut DependencyTree,
    ) -> Result<Vec<PathBuf>, Error> {
        let dependent_file: PathBuf = file.into();
        if tree.contains(&dependent_file) {
            return Ok(Default::default());
        }
        let mut dependencies: Vec<PathBuf> = Vec::new();
        let mut file = File::open(&dependent_file)?;
        let elf = Elf::read(&mut file, self.page_size)?;
        let names = elf.read_section_names(&mut file)?.unwrap_or_default();
        let dynstr_table = elf
            .read_dynamic_string_table(&mut file)?
            .unwrap_or_default();
        let Some(dynamic_table) = elf.read_dynamic_table(&mut file)? else {
            // No dependencies.
            tree.insert(dependent_file, Default::default());
            return Ok(Default::default());
        };
        let interpreter = elf
            .read_interpreter(&names, &mut file)?
            .map(|c_str| PathBuf::from(OsString::from_vec(c_str.into_bytes())));
        let mut search_dirs = Vec::new();
        let runpath = dynamic_table.get(DynamicTag::Runpath);
        let rpath = dynamic_table.get(DynamicTag::Rpath);
        let override_dirs = match self.libc {
            Libc::Glibc => runpath.is_some(),
            Libc::Musl => true,
        };
        if override_dirs {
            // Directories that are searched before RUNPATH/RPATH.
            search_dirs.extend_from_slice(self.search_dirs_override.as_slice());
        }
        let mut extend_search_dirs = |path: &CStr| {
            search_dirs.extend(split_paths(OsStr::from_bytes(path.to_bytes())).map(|dir| {
                interpolate(
                    &dir,
                    &dependent_file,
                    &elf,
                    self.lib.as_deref(),
                    self.platform.as_deref(),
                )
            }));
        };
        match self.libc {
            Libc::Glibc => {
                // Try RUNPATH first.
                runpath
                    .and_then(|string_offset| dynstr_table.get_string(string_offset as usize))
                    .map(&mut extend_search_dirs)
                    .or_else(|| {
                        // Otherwise try RPATH.
                        //
                        // Note that GNU ld.so searches dependent's RPATH, then dependent of the dependent's
                        // RPATH and so on *before* it searches RPATH of the executable itself. This goes
                        // against simplistic design of this dynamic loader, and hopefully noone uses this
                        // deprecated functionality.
                        rpath
                            .and_then(|string_offset| {
                                dynstr_table.get_string(string_offset as usize)
                            })
                            .map(&mut extend_search_dirs)
                    });
            }
            Libc::Musl => [rpath, runpath]
                .into_iter()
                .flatten()
                .filter_map(|string_offset| dynstr_table.get_string(string_offset as usize))
                .for_each(&mut extend_search_dirs),
        }
        // Directories that are searched after RUNPATH or RPATH.
        search_dirs.extend_from_slice(self.search_dirs.as_slice());
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
                    if Some(path.as_path()) != interpreter.as_deref() {
                        dependencies.push(path);
                    }
                    continue 'outer;
                }
            }
            return Err(Error::FailedToResolve(dep_name.into(), dependent_file));
        }
        if let Some(interpreter) = interpreter {
            if !dependencies.contains(&interpreter) {
                dependencies.push(interpreter);
            }
        }
        tree.insert(dependent_file, dependencies.clone());
        dependencies.retain(|dep| !tree.contains(dep));
        Ok(dependencies)
    }
}

/// Get library search directories from the environment variables.
///
/// These directories override default search directories unless an executable has `RPATH`.
///
/// Uses `LD_LIBRARY_PATH` environemnt variable.
pub fn get_search_dirs_from_env() -> Vec<PathBuf> {
    std::env::var_os("LD_LIBRARY_PATH")
        .map(|path| split_paths(&path).collect())
        .unwrap_or_default()
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
