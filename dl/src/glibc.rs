use std::collections::VecDeque;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use crate::fs::File;
use glob::glob;
use log::log_enabled;
use log::trace;
use log::warn;
use log::Level::Trace;

/// Get default library search directories plus the paths from `<rootfs_dir>/etc/ld.so.conf`.
///
/// Default search directories: `/lib:/usr/local/lib:/usr/lib`.
pub fn get_search_dirs<P: AsRef<Path>>(rootfs_dir: P) -> Result<Vec<PathBuf>, std::io::Error> {
    let rootfs_dir = rootfs_dir.as_ref();
    let mut paths = Vec::new();
    paths.extend([
        rootfs_dir.join("lib"),
        rootfs_dir.join("usr/local/lib"),
        rootfs_dir.join("usr/lib"),
    ]);
    parse_ld_so_conf(rootfs_dir.join("etc/ld.so.conf"), rootfs_dir, &mut paths)?;
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
                let pattern = if line.as_bytes().get(i + 1).copied() == Some(b'/') {
                    &line[i + 2..]
                } else {
                    &line[i + 1..]
                };
                let pattern = rootfs_dir.join(pattern);
                let Some(pattern) = pattern.to_str() else {
                    // Not a valid UTF-8 string.
                    continue;
                };
                let Ok(more_paths) = glob(pattern) else {
                    // Unparsable glob pattern.
                    continue;
                };
                for path in more_paths {
                    let Ok(path) = path else {
                        continue;
                    };
                    if !conf_files.contains(&path) {
                        queue.push_back(path);
                    }
                }
            }
            if let Some(path) = line.strip_prefix("/") {
                let path = rootfs_dir.join(path);
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    Ok(())
}

/// Get library search directories from via `ld.so --list-diagnostics`.
///
/// Useful for Nix and Guix.
pub fn get_hard_coded_search_dirs(
    ld_so_exe: Option<Command>,
) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut child = ld_so_exe
        .unwrap_or_else(|| Command::new("ld.so"))
        .arg("--list-diagnostics")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut paths = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if !line.starts_with("path.system_dirs") {
                continue;
            }
            let Some(i) = line.find('=') else {
                continue;
            };
            let mut start = i + 1;
            let mut end = line.len() - 1;
            // Remove quotes.
            if line.as_bytes().get(i + 1) == Some(&b'"') {
                start += 1;
            }
            if line.as_bytes().last() == Some(&b'"') {
                end -= 1;
            }
            let path = &line[start..end];
            paths.push(Path::new(path).to_path_buf());
        }
    }
    Ok(paths)
}
