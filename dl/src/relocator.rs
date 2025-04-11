use std::collections::HashMap;
use std::collections::VecDeque;
use std::ffi::CString;
use std::ffi::OsStr;
use std::fs::Permissions;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use elb::DynamicTag;
use elb::Elf;
use elb::ElfPatcher;
use sha2::Digest;
use sha2::Sha256;

use crate::base32;
use crate::fs;
use crate::fs::os::unix::fs::symlink;
use crate::DependencyTree;
use crate::DynamicLoader;
use crate::Error;

/// Relocates ELF together with its dependencies.
pub struct ElfRelocator {
    loader: DynamicLoader,
}

impl ElfRelocator {
    /// Create new relocator that uses the specified dynamic loader.
    pub fn new(loader: DynamicLoader) -> Self {
        Self { loader }
    }

    /// Relocates ELF `file` to `directory` together with its dependencies.
    ///
    /// Each ELF is copied to the subdirectory which name is BASE32-encoded hash of the file. The
    /// dependencies are then symlinked into this directory. Each ELF's `RUNPATH` is
    /// set to the containing directory. Each ELF's interpreter is changed to point to the interpreter from that
    /// directory. All executables are symlinked into `directory/bin`.
    pub fn relocate<P1: Into<PathBuf>, P2: AsRef<Path>>(
        &self,
        file: P1,
        directory: P2,
    ) -> Result<PathBuf, Error> {
        let file = file.into();
        let directory = directory.as_ref();
        let mut tree = DependencyTree::new();
        let mut queue = VecDeque::new();
        queue.push_back(file.clone());
        while let Some(file) = queue.pop_front() {
            let dependencies = self.loader.resolve_dependencies(&file, &mut tree)?;
            queue.extend(dependencies);
        }
        let mut hashes = HashMap::with_capacity(tree.len());
        for (dependent, _dependencies) in tree.iter() {
            let (hash, new_path) = relocate_file(dependent, directory)?;
            patch_file(&new_path, directory, &hash, self.loader.page_size)?;
            // TODO The hash is not updated after patching.
            hashes.insert(dependent.clone(), hash);
        }
        for (dependent, dependencies) in tree.iter() {
            let hash = hashes.get(dependent).expect("Inserted above");
            let dir = directory.join(hash.as_str());
            for dep in dependencies.iter() {
                let file_name = dep.file_name().expect("File name exists");
                let dep_hash = hashes.get(dep).expect("Inserted above");
                let source = {
                    let mut path = PathBuf::new();
                    path.push("..");
                    path.push(dep_hash.as_str());
                    path.push(file_name);
                    path
                };
                let target = dir.join(file_name);
                let _ = std::fs::remove_file(&target);
                symlink(&source, &target)?;
            }
        }
        let mut new_path = PathBuf::new();
        new_path.push(directory);
        new_path.push(hashes.get(&file).expect("Inserted above").as_str());
        new_path.push(file.file_name().expect("File name exists"));
        Ok(new_path)
    }
}

fn relocate_file(file: &Path, dir: &Path) -> Result<(Hash, PathBuf), Error> {
    let hash = {
        let mut file = fs::File::open(file)?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)?;
        let hash = hasher.finalize();
        let mut encoded_hash: HashArray = [0_u8; base32::encoded_len(32)];
        base32::encode_into(&hash[..], &mut encoded_hash[..]);
        Hash(encoded_hash)
    };
    let mut new_path = PathBuf::new();
    new_path.push(dir);
    new_path.push(hash.as_str());
    fs::create_dir_all(&new_path)?;
    new_path.push(file.file_name().expect("File name exists"));
    let _ = std::fs::remove_file(&new_path);
    fs::copy(file, &new_path)?;
    Ok((hash, new_path))
}

fn patch_file(file: &Path, directory: &Path, hash: &Hash, page_size: u64) -> Result<(), Error> {
    let dir = file.parent().expect("Parent directory exists");
    let dir_bytes = dir.as_os_str().as_bytes();
    let file_name = file.file_name().expect("File name exists");
    let Some(file_kind) = get_file_kind(file, page_size)? else {
        // Don't patch weird files.
        return Ok(());
    };
    let mode = match file_kind {
        FileKind::Executable | FileKind::Static => 0o755,
        FileKind::Library => 0o644,
    };
    fs::set_permissions(file, Permissions::from_mode(mode))?;
    if matches!(file_kind, FileKind::Executable | FileKind::Static) {
        let bin = directory.join("bin");
        fs::create_dir_all(&bin)?;
        let source = {
            let mut path = PathBuf::new();
            path.push("..");
            path.push(hash.as_str());
            path.push(file_name);
            path
        };
        let target = bin.join(file_name);
        let _ = std::fs::remove_file(&target);
        symlink(&source, &target)?;
    }
    if file_kind == FileKind::Static {
        // Don't patch statically-linked executables.
        return Ok(());
    }
    let mut file = fs::OpenOptions::new().read(true).write(true).open(file)?;
    let elf = Elf::read(&mut file, page_size)?;
    let mut patcher = ElfPatcher::new(elf, file);
    if let Some(old_interpreter) = patcher.read_interpreter()? {
        let interpreter = {
            let old_interpreter = Path::new(OsStr::from_bytes(old_interpreter.to_bytes()));
            let file_name_bytes = old_interpreter
                .file_name()
                .expect("File name exists")
                .as_bytes();
            let mut bytes = Vec::with_capacity(dir_bytes.len() + file_name_bytes.len() + 2);
            bytes.extend_from_slice(dir_bytes);
            bytes.push(b'/');
            bytes.extend_from_slice(file_name_bytes);
            bytes.push(0_u8);
            unsafe { CString::from_vec_with_nul_unchecked(bytes) }
        };
        patcher.set_interpreter(interpreter.as_c_str())?;
    }
    let runpath = {
        let mut bytes = Vec::with_capacity(dir_bytes.len() + 1);
        bytes.extend_from_slice(dir_bytes);
        bytes.push(0_u8);
        unsafe { CString::from_vec_with_nul_unchecked(bytes) }
    };
    patcher.set_library_search_path(DynamicTag::Runpath, runpath.as_c_str())?;
    patcher.finish()?;
    Ok(())
}

fn get_file_kind(file: &Path, page_size: u64) -> Result<Option<FileKind>, Error> {
    let mut file = fs::File::open(file)?;
    let elf = Elf::read(&mut file, page_size)?;
    if elf.read_interpreter(&mut file)?.is_some() {
        return Ok(Some(FileKind::Executable));
    }
    // No interpreter, but may be this is statically-linked executable.
    let Some(dynamic_table) = elf.read_dynamic_table(&mut file)? else {
        return Ok(None);
    };
    Ok(match dynamic_table.get(DynamicTag::Needed) {
        Some(..) => Some(FileKind::Library),
        None => Some(FileKind::Static),
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FileKind {
    Executable,
    Library,
    Static,
}

type HashArray = [u8; base32::encoded_len(32)];

struct Hash(HashArray);

impl Hash {
    fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(&self.0[..]) }
    }
}
