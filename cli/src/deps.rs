use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::env::split_paths;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use elb_dl::glibc;
use elb_dl::musl;
use elb_dl::DynamicLoader;

use crate::CommonArgs;

#[derive(clap::Args)]
pub struct DepsArgs {
    /// File system root.
    #[clap(short = 'r', long = "root", value_name = "DIR", default_value = "/")]
    root: PathBuf,

    /// Which architecture to use.
    ///
    /// This value is used to interpolate `$PLATFORM` in RPATH/RUNPATH.
    #[clap(long = "arch", value_name = "ARCH")]
    arch: Option<String>,

    /// Override library search directories.
    #[clap(short = 'L', long = "search-dirs", value_name = "DIR1:DIR2:...")]
    search_dirs: Option<PathBuf>,

    /// Use `ld.so --list-diagnostics` to figure out hard-coded library search directoris.
    ///
    /// Useful on Guix and Nix.
    #[clap(action, long = "hard-coded-search-dirs")]
    hard_coded_search_dirs: bool,

    /// Tree visual style.
    #[clap(
        short = 's',
        long = "style",
        value_name = "STYLE",
        default_value = "rounded"
    )]
    style: TreeStyleKind,

    /// Data output format.
    #[clap(
        short = 'f',
        long = "format",
        value_name = "FORMAT",
        default_value = "tree"
    )]
    format: DepsFormat,

    /// Which libc implementation to emulate.
    #[clap(
        short = 'l',
        long = "libc",
        value_name = "LIBC",
        default_value = "glibc"
    )]
    libc: Libc,

    /// ELF file(s).
    #[clap(value_name = "FILE...")]
    files: Vec<PathBuf>,
}

pub fn deps(common: CommonArgs, args: DepsArgs) -> Result<(), Box<dyn std::error::Error>> {
    let search_dirs = {
        let mut search_dirs = Vec::new();
        if let Some(path) = args.search_dirs.as_ref() {
            // Custom library search directories.
            search_dirs.extend(split_paths(path));
        } else {
            // Add directories from the environment.
            if let Some(path) = std::env::var_os("LD_LIBRARY_PATH") {
                search_dirs.extend(split_paths(&path));
            }
            // Add system directories.
            match args.libc {
                Libc::Glibc => {
                    search_dirs.extend(glibc::get_search_dirs(&args.root)?);
                    if args.hard_coded_search_dirs {
                        let ld_so = if args.root == Path::new("/") {
                            None
                        } else {
                            Some(Command::new(args.root.join("bin/ls")))
                        };
                        search_dirs.extend(glibc::get_hard_coded_search_dirs(ld_so)?);
                    }
                }
                Libc::Musl => {
                    let arch = args.arch.as_deref().unwrap_or(std::env::consts::ARCH);
                    search_dirs.extend(musl::get_search_dirs(&args.root, arch)?);
                }
            }
        }
        search_dirs
    };
    let loader = DynamicLoader::options()
        .page_size(common.page_size)
        .search_dirs(search_dirs)
        .platform(args.arch.map(|x| x.into()))
        .new_loader();
    let mut table: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    let mut queue = VecDeque::new();
    for file in args.files.iter() {
        for path in loader.resolve_dependencies(file)?.1.into_iter() {
            let path = fs_err::canonicalize(path)?;
            queue.push_back((file.clone(), path));
        }
        while let Some((dependent, dependency)) = queue.pop_front() {
            if !table
                .entry(dependent.clone())
                .or_default()
                .insert(dependency.clone())
            {
                continue;
            }
            for path in loader.resolve_dependencies(&dependency)?.1.into_iter() {
                let path = fs_err::canonicalize(path)?;
                queue.push_back((dependency.clone(), path));
            }
        }
    }
    let mut writer = BufWriter::new(std::io::stdout());
    let style = args.style.to_style();
    match args.format {
        DepsFormat::List => {
            let mut all_dependencies = BTreeSet::new();
            for file in args.files.iter() {
                all_dependencies.extend(table.remove(file).unwrap_or_default());
            }
            for (dependent, dependencies) in table.into_iter() {
                all_dependencies.insert(dependent);
                all_dependencies.extend(dependencies);
            }
            for dep in all_dependencies.into_iter() {
                writeln!(writer, "{}", dep.display())?;
            }
        }
        DepsFormat::Tree => {
            for file in args.files.into_iter() {
                let last = table.len() == 1;
                let mut stack = VecDeque::new();
                stack.push_back(last);
                print_tree(&mut writer, &mut stack, file, &table, style)?;
            }
        }
        DepsFormat::TableTree => {
            for (dependent, dependencies) in table.into_iter() {
                let last = true;
                let mut stack = VecDeque::new();
                stack.push_back(last);
                let mut table = BTreeMap::new();
                table.insert(dependent.clone(), dependencies);
                print_tree(&mut writer, &mut stack, dependent.clone(), &table, style)?;
            }
        }
    }
    writer.flush()?;
    Ok(())
}

fn print_tree<W: Write>(
    writer: &mut W,
    stack: &mut VecDeque<bool>,
    node: PathBuf,
    table: &BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    style: TreeStyle,
) -> Result<(), std::io::Error> {
    let mut prev_last = stack.iter().skip(1).copied().next().unwrap_or(false);
    for last in stack.iter().skip(2).copied() {
        if prev_last {
            write!(writer, "    ")?;
        } else {
            write!(writer, " {}  ", style.0[2])?;
        }
        prev_last = last;
    }
    if stack.len() > 1 {
        let last = stack.iter().last().copied().unwrap_or(false);
        let ch = if last { style.0[0] } else { style.0[3] };
        write!(writer, " {}{}{} ", ch, style.0[1], style.0[1])?;
    }
    writeln!(writer, "{}", node.display())?;
    let Some(children) = table.get(&node) else {
        return Ok(());
    };
    for (i, child) in children.iter().enumerate() {
        let last = i == children.len() - 1;
        stack.push_back(last);
        print_tree(writer, stack, child.clone(), table, style)?;
        stack.pop_back();
    }
    Ok(())
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum Libc {
    Glibc,
    Musl,
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum DepsFormat {
    List,
    Tree,
    TableTree,
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum TreeStyleKind {
    Ascii,
    Rounded,
}

impl TreeStyleKind {
    fn to_style(self) -> TreeStyle {
        match self {
            Self::Ascii => TREE_STYLE_ASCII,
            Self::Rounded => TREE_STYLE_ROUNDED,
        }
    }
}

#[derive(Clone, Copy)]
struct TreeStyle([char; 4]);

const TREE_STYLE_ASCII: TreeStyle = TreeStyle(['\\', '_', '|', '|']);
const TREE_STYLE_ROUNDED: TreeStyle = TreeStyle(['╰', '─', '│', '├']);
