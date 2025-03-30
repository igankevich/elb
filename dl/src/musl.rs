use std::env::split_paths;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;

use fs_err::File;
use log::log_enabled;
use log::trace;
use log::warn;
use log::Level::Trace;

/// Get library search directories from via `<rootfs_dir>/etc/ld-musl-<arch>.path`.
///
/// If the file is empty, returns default search directories: `/lib:/usr/local/lib:/usr/lib`.
pub fn get_search_dirs<P: AsRef<Path>>(
    rootfs_dir: P,
    arch: &str,
) -> Result<Vec<PathBuf>, std::io::Error> {
    let rootfs_dir = rootfs_dir.as_ref();
    let mut paths = Vec::new();
    parse_paths(
        rootfs_dir.join(format!("etc/ld-musl-{arch}.path")),
        rootfs_dir,
        &mut paths,
    )?;
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

fn parse_paths(
    path: PathBuf,
    rootfs_dir: &Path,
    paths: &mut Vec<PathBuf>,
) -> Result<(), std::io::Error> {
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(ref e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            warn!("Failed to open {path:?}: {e}");
            return Ok(());
        }
    };
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        for path in split_paths(line) {
            let path = match path.strip_prefix("/") {
                Ok(path) => path,
                Err(_) => path.as_path(),
            };
            paths.push(rootfs_dir.join(path));
        }
    }
    Ok(())
}
