use clap::ValueEnum;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use elb::Elf;
use elb::ElfPatcher;
use fs_err::File;
use fs_err::OpenOptions;

use crate::CommonArgs;

#[derive(clap::Args)]
pub struct PatchArgs {
    /// Set interpreter.
    #[clap(long = "set-interpreter", value_name = "file")]
    set_interpreter: Option<PathBuf>,

    /// Remove interpreter.
    #[clap(action, long = "remove-interpreter")]
    remove_interpreter: bool,

    /// Set dynamic table entry.
    #[clap(long = "set-dynamic", value_name = "tag=value,...")]
    set_dynamic: Vec<String>,

    /// Remove dynamic table entry.
    #[clap(action, long = "remove-dynamic")]
    remove_dynamic: Vec<DynamicEntry>,

    /// ELF file.
    #[clap(value_name = "ELF file")]
    file: PathBuf,
}

pub fn patch(common: CommonArgs, args: PatchArgs) -> Result<(), Box<dyn std::error::Error>> {
    let elf = Elf::read(&mut File::open(&args.file)?, common.page_size)?;
    let mut changed = false;
    let file_name = args.file.file_name().expect("File name exists");
    let new_file_name = {
        let mut name = OsString::new();
        name.push(".");
        name.push(file_name);
        name.push(".tmp");
        name
    };
    let new_path = match args.file.parent() {
        Some(parent) => parent.join(&new_file_name),
        None => new_file_name.into(),
    };
    let _ = std::fs::remove_file(&new_path);
    fs_err::copy(&args.file, &new_path)?;
    let file = OpenOptions::new().read(true).write(true).open(&new_path)?;
    let mut patcher = ElfPatcher::new(elf, file);
    if args.remove_interpreter {
        patcher.remove_interpreter()?;
        changed = true;
    } else if let Some(path) = args.set_interpreter {
        let os_string = path.into_os_string();
        let mut bytes = os_string.into_vec();
        bytes.push(0_u8);
        let c_str = CStr::from_bytes_with_nul(&bytes)?;
        patcher.set_interpreter(c_str)?;
        changed = true;
    }
    for entry in args.remove_dynamic.into_iter() {
        patcher.remove_dynamic_tag(entry.into())?;
        changed = true;
    }
    for pair in args.set_dynamic.into_iter() {
        let mut iter = pair.splitn(2, '=');
        let tag: DynamicEntry = ValueEnum::from_str(iter.next().ok_or("Tag not found")?, true)?;
        let mut value = iter.next().ok_or("Value not found")?.as_bytes().to_vec();
        value.push(0_u8);
        let value = CString::from_vec_with_nul(value)?;
        if !matches!(tag, DynamicEntry::Rpath | DynamicEntry::Runpath) {
            return Err("Only RUNPATH and RPATH can be set".into());
        }
        patcher.set_library_search_path(tag.into(), value.as_c_str())?;
        changed = true;
    }
    if !changed {
        return Err("No changes".into());
    }
    patcher.finish()?;
    fs_err::rename(&new_path, &args.file)?;
    Ok(())
}

#[derive(clap::ValueEnum, Clone, Copy)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
enum DynamicEntry {
    Rpath,
    Runpath,
}

impl From<DynamicEntry> for elb::DynamicTag {
    fn from(other: DynamicEntry) -> Self {
        match other {
            DynamicEntry::Rpath => Self::Rpath,
            DynamicEntry::Runpath => Self::Runpath,
        }
    }
}
