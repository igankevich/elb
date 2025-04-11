use std::collections::VecDeque;
use std::env::split_paths;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use elb_dl::glibc;
use elb_dl::musl;
use elb_dl::DependencyTree;
use elb_dl::DynamicLoader;

use crate::CommonArgs;

#[derive(clap::Args)]
pub struct LoaderArgs {
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

    /// Use `ld.so --list-diagnostics` to figure out hard-coded library search directories.
    ///
    /// Useful on Guix and Nix.
    #[clap(action, long = "hard-coded-search-dirs")]
    hard_coded_search_dirs: bool,

    /// Which libc implementation to emulate.
    ///
    /// This affects default library search paths and library search order.
    #[clap(
        short = 'l',
        long = "libc",
        value_name = "LIBC",
        default_value = "glibc"
    )]
    libc: Libc,
}

impl LoaderArgs {
    fn search_dirs(&self) -> Result<Vec<PathBuf>, elb_dl::Error> {
        let mut search_dirs = Vec::new();
        if let Some(path) = self.search_dirs.as_ref() {
            // Custom library search directories.
            search_dirs.extend(split_paths(path));
        } else {
            // Add system directories.
            match self.libc {
                Libc::Glibc => {
                    search_dirs.extend(glibc::get_search_dirs(&self.root)?);
                    if self.hard_coded_search_dirs {
                        let ld_so = if self.root == Path::new("/") {
                            None
                        } else {
                            Some(Command::new(self.root.join("bin/ld.so")))
                        };
                        search_dirs.extend(glibc::get_hard_coded_search_dirs(ld_so)?);
                    }
                }
                Libc::Musl => {
                    let arch = self.arch.as_deref().unwrap_or(std::env::consts::ARCH);
                    search_dirs.extend(musl::get_search_dirs(&self.root, arch)?);
                }
            }
        }
        Ok(search_dirs)
    }

    pub fn new_loader(self, page_size: u64) -> Result<DynamicLoader, elb_dl::Error> {
        let search_dirs = self.search_dirs()?;
        let loader = DynamicLoader::options()
            .libc(self.libc.into())
            .page_size(page_size)
            .search_dirs_override(
                std::env::var_os("LD_LIBRARY_PATH")
                    .map(|path| split_paths(&path).collect())
                    .unwrap_or_default(),
            )
            .search_dirs(search_dirs)
            .platform(self.arch.map(|x| x.into()))
            .new_loader();
        Ok(loader)
    }
}

#[derive(clap::Args)]
pub struct DepsArgs {
    #[clap(flatten)]
    loader: LoaderArgs,

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

    /// Print file names instead of full paths.
    #[clap(action, short = 'n', long = "names-only")]
    names_only: bool,

    /// ELF file(s).
    #[clap(value_name = "FILE...")]
    files: Vec<PathBuf>,
}

pub fn deps(common: CommonArgs, args: DepsArgs) -> Result<(), Box<dyn std::error::Error>> {
    let loader = args.loader.new_loader(common.page_size)?;
    let mut tree = DependencyTree::new();
    let mut queue = VecDeque::new();
    queue.extend(args.files.iter().cloned());
    while let Some(file) = queue.pop_front() {
        let dependencies = loader.resolve_dependencies(&file, &mut tree)?;
        queue.extend(dependencies);
    }
    let mut writer = BufWriter::new(std::io::stdout());
    let style = args.style.to_style();
    match args.format {
        DepsFormat::List => {
            let mut all_dependencies = Vec::new();
            for file in args.files.iter() {
                all_dependencies.extend(tree.remove(file).unwrap_or_default());
            }
            for (dependent, dependencies) in tree.into_iter() {
                all_dependencies.push(dependent);
                all_dependencies.extend(dependencies);
            }
            all_dependencies.sort_unstable();
            all_dependencies.dedup();
            for dep in all_dependencies.into_iter() {
                let name = if args.names_only {
                    dep.file_name()
                        .map(Path::new)
                        .unwrap_or_else(|| dep.as_path())
                } else {
                    dep.as_path()
                };
                writeln!(writer, "{}", name.display())?;
            }
        }
        DepsFormat::Tree => {
            for file in args.files.into_iter() {
                let last = tree.len() == 1;
                let mut stack = VecDeque::new();
                stack.push_back(last);
                print_tree(&mut writer, &mut stack, file, &tree, style, args.names_only)?;
            }
        }
        DepsFormat::TableTree => {
            for (dependent, dependencies) in tree.into_iter() {
                let last = true;
                let mut stack = VecDeque::new();
                stack.push_back(last);
                let mut tree = DependencyTree::new();
                tree.insert(dependent.clone(), dependencies);
                print_tree(
                    &mut writer,
                    &mut stack,
                    dependent.clone(),
                    &tree,
                    style,
                    args.names_only,
                )?;
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
    tree: &DependencyTree,
    style: TreeStyle,
    names_only: bool,
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
    let name = if names_only {
        node.file_name()
            .map(Path::new)
            .unwrap_or_else(|| node.as_path())
    } else {
        node.as_path()
    };
    writeln!(writer, "{}", name.display())?;
    let Some(children) = tree.get(&node) else {
        return Ok(());
    };
    for (i, child) in children.iter().enumerate() {
        let last = i == children.len() - 1;
        stack.push_back(last);
        print_tree(writer, stack, child.clone(), tree, style, names_only)?;
        stack.pop_back();
    }
    Ok(())
}

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum Libc {
    Glibc,
    Musl,
}

impl From<Libc> for elb_dl::Libc {
    fn from(other: Libc) -> Self {
        match other {
            Libc::Glibc => Self::Glibc,
            Libc::Musl => Self::Musl,
        }
    }
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
