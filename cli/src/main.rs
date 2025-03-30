use clap::Parser;
use clap::ValueEnum;
use colored::Colorize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::env::split_paths;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsString;
use std::io::BufWriter;
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::process::ExitCode;

use elfie::ArmFlags;
use elfie::Elf;
use elfie::ElfPatcher;
use elfie::Machine;
use elfie::StringTable;
use elfie_dl::glibc;
use elfie_dl::musl;
use elfie_dl::DynamicLoader;
use fs_err::File;
use fs_err::OpenOptions;

mod formatting;
mod logger;

use self::formatting::*;
use self::logger::*;

#[derive(clap::Parser)]
#[clap(version)]
struct Args {
    /// Verbose output.
    #[clap(short = 'v', long = "verbose")]
    verbose: bool,

    #[clap(flatten)]
    common: CommonArgs,

    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Show file contents.
    Show {
        /// What to show?
        #[clap(short = 't', default_value = "all")]
        what: What,

        /// ELF file.
        #[clap(value_name = "ELF file")]
        file: PathBuf,
    },
    /// Validate the file.
    Check {
        /// ELF file.
        #[clap(value_name = "ELF file")]
        file: PathBuf,
    },
    /// Print dependencies.
    Deps(DepsArgs),
    /// Modify ELF file.
    Patch(PatchArgs),
}

#[derive(clap::Args)]
struct CommonArgs {
    /// Memory page size.
    #[clap(long = "page-size", value_name = "NUM", default_value_t = 4096)]
    page_size: u64,
}

#[derive(clap::Args)]
struct DepsArgs {
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

    /// ELF file.
    #[clap(value_name = "ELF file")]
    file: PathBuf,
}

#[derive(clap::Args)]
struct PatchArgs {
    /// Set interpreter.
    #[clap(long = "set-interpreter", value_name = "file")]
    set_interpreter: Option<PathBuf>,

    /// Remove interpreter.
    #[clap(action, long = "remove-interpreter")]
    remove_interpreter: bool,

    /// Set dynamic table entry.
    #[clap(long = "add-dynamic", value_name = "tag=value,...")]
    set_dynamic: Vec<String>,

    /// Remove dynamic table entry.
    #[clap(action, long = "remove-dynamic")]
    remove_dynamic: Vec<DynamicEntry>,

    /// ELF file.
    #[clap(value_name = "ELF file")]
    file: PathBuf,
}

fn main() -> ExitCode {
    match do_main() {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn do_main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    Logger::init(args.verbose)?;
    match args.command {
        Command::Show { what, file } => show(args.common, what, file),
        Command::Check { file } => check(args.common, file),
        Command::Deps(deps_args) => deps(args.common, deps_args),
        Command::Patch(patch_args) => patch(args.common, patch_args),
    }
}

fn show(common: CommonArgs, what: What, file: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&file)?;
    let elf = Elf::read_unchecked(&mut file, common.page_size)?;
    let section_names = elf.read_section_names(&mut file)?.unwrap_or_default();
    match what {
        What::Header => {
            let mut printer = Printer::new(false);
            show_header(&elf, &mut printer);
        }
        What::Sections => {
            let mut printer = Printer::new(false);
            show_sections(&elf, &section_names, &mut printer)?;
        }
        What::Segments => {
            let mut printer = Printer::new(false);
            show_segments(&elf, &section_names, &mut printer)?;
        }
        What::All => {
            let mut printer = Printer::new(true);
            printer.title("Header");
            show_header(&elf, &mut printer);
            printer.title("Sections");
            show_sections(&elf, &section_names, &mut printer)?;
            printer.title("Segments");
            show_segments(&elf, &section_names, &mut printer)?;
        }
    }
    elf.check()?;
    Ok(())
}

fn show_header(elf: &Elf, printer: &mut Printer) {
    printer.kv("Class", format_args!("{:?}", elf.header.class));
    printer.kv("Byte order", format_args!("{:?}", elf.header.byte_order));
    printer.kv("OS ABI", format_args!("{:?}", elf.header.os_abi));
    printer.kv("ABI version", format_args!("{:?}", elf.header.abi_version));
    printer.kv("File type", format_args!("{:?}", elf.header.kind));
    printer.kv("Machine", format_args!("{:?}", elf.header.machine));
    match elf.header.machine {
        Machine::Arm => {
            let arm_flags = ArmFlags::from_bits_retain(elf.header.flags);
            printer.kv(
                "Flags",
                format_args!("{:?} ({:#x})", arm_flags, elf.header.flags,),
            );
        }
        _ => printer.kv("Flags", format_args!("{:#x}", elf.header.flags)),
    }
    printer.kv("Entry point", format_args!("{:#x}", elf.header.entry_point));
    printer.kv(
        "Program header",
        format_args!(
            "{:#x}..{:#x}",
            elf.header.program_header_offset,
            elf.header.program_header_offset
                + elf.header.num_segments as u64 * elf.header.segment_len as u64
        ),
    );
    printer.kv(
        "Section header",
        format_args!(
            "{:#x}..{:#x}",
            elf.header.section_header_offset,
            elf.header.section_header_offset
                + elf.header.num_sections as u64 * elf.header.section_len as u64
        ),
    );
}

fn show_sections(
    elf: &Elf,
    names: &StringTable,
    printer: &mut Printer,
) -> Result<(), Box<dyn std::error::Error>> {
    if !elf.sections.is_empty() {
        printer.row(format_args!(
            "{:20}  {:38}  {:38}  Flags      Type",
            "Name", "File block", "Memory block"
        ));
    }
    for section in elf.sections.iter() {
        let memory_start = section.virtual_address;
        let memory_end = memory_start + section.size;
        let file_start = section.offset;
        let file_end = file_start + section.size;
        let name_bytes = names
            .get_string(section.name_offset as usize)
            .unwrap_or_default();
        let name = String::from_utf8_lossy(name_bytes.to_bytes());
        printer.row(format_args!(
            "{:20}  {:#018x}..{:#018x}  {:#018x}..{:#018x}  {}  {}",
            name,
            file_start,
            file_end,
            memory_start,
            memory_end,
            SectionFlagsStr(section.flags),
            SectionKindStr(section.kind)
        ));
    }
    printer.title("Section flags");
    println!("  w  Writable");
    println!("  a  Occupies memory during execution");
    println!("  x  Executable");
    println!("  m  Mergeable");
    println!("  s  Contains NUL-terminated strings");
    println!("  i  Linked to another section");
    println!("  l  Preserve order after combining");
    println!("  o  OS specific handling required");
    println!("  g  Group member");
    println!("  t  Holds thread-local data");
    println!("  c  Compressed");
    println!("  *  Unknown flags");
    Ok(())
}

fn show_segments(
    elf: &Elf,
    names: &StringTable,
    printer: &mut Printer,
) -> Result<(), Box<dyn std::error::Error>> {
    if !elf.sections.is_empty() {
        printer.row(format_args!(
            "{:20}  {:38}  {:38}  Flags  Sections",
            "Type", "File block", "Memory block"
        ));
    }
    for segment in elf.segments.iter() {
        let memory_start = segment.virtual_address;
        let memory_end = memory_start + segment.memory_size;
        let file_start = segment.offset;
        let file_end = file_start + segment.file_size;
        let mut section_names = Vec::new();
        for section in elf.sections.iter() {
            if (file_start..file_end).contains(&section.offset) {
                let name_bytes = names
                    .get_string(section.name_offset as usize)
                    .unwrap_or_default();
                let name = String::from_utf8_lossy(name_bytes.to_bytes());
                if name.is_empty() {
                    continue;
                }
                section_names.push(name);
            }
        }
        printer.row(format_args!(
            "{:20}  {:#018x}..{:#018x}  {:#018x}..{:#018x}  {}  {}",
            SegmentKindStr(segment.kind),
            file_start,
            file_end,
            memory_start,
            memory_end,
            SegmentFlagsStr(segment.flags),
            section_names.join(" ")
        ));
    }
    Ok(())
}

fn check(common: CommonArgs, file: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&file)?;
    let _elf = Elf::read(&mut file, common.page_size)?;
    Ok(())
}

fn deps(common: CommonArgs, args: DepsArgs) -> Result<(), Box<dyn std::error::Error>> {
    let search_dirs = {
        let mut search_dirs = Vec::new();
        match args.libc {
            Libc::Glibc => search_dirs.extend(glibc::get_search_dirs(&args.root)?),
            Libc::Musl => {
                let arch = args.arch.as_deref().unwrap_or(std::env::consts::ARCH);
                search_dirs.extend(musl::get_search_dirs(&args.root, arch)?);
            }
        }
        if let Some(path) = args.search_dirs.as_ref() {
            search_dirs.extend(split_paths(path));
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
    for path in loader.resolve_dependencies(&args.file)?.1.into_iter() {
        let path = fs_err::canonicalize(path)?;
        queue.push_back((args.file.clone(), path));
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
    let style = args.style.to_style();
    let mut writer = BufWriter::new(std::io::stdout());
    match args.format {
        DepsFormat::List => {
            let mut all_dependencies = BTreeSet::new();
            all_dependencies.extend(table.remove(&args.file).unwrap_or_default());
            for (dependent, dependencies) in table.into_iter() {
                all_dependencies.insert(dependent);
                all_dependencies.extend(dependencies);
            }
            for dep in all_dependencies.into_iter() {
                writeln!(writer, "{}", dep.display())?;
            }
        }
        DepsFormat::Tree => {
            let last = table.len() == 1;
            let mut stack = VecDeque::new();
            stack.push_back(last);
            print_tree(&mut writer, &mut stack, args.file.clone(), &table, style)?;
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

fn patch(common: CommonArgs, args: PatchArgs) -> Result<(), Box<dyn std::error::Error>> {
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

struct Printer {
    first_title: bool,
    indent: bool,
}

impl Printer {
    fn new(indent: bool) -> Self {
        Self {
            first_title: true,
            indent,
        }
    }

    fn title(&mut self, title: &str) {
        let newline = if !self.first_title {
            "\n"
        } else {
            self.first_title = false;
            ""
        };
        println!("{}{}", newline, title.bold().underline());
    }

    fn kv<V: std::fmt::Display>(&mut self, key: &str, value: V) {
        let indent = if self.indent { "  " } else { "" };
        println!("{}{}: {}", indent, key.bold().blue(), value);
    }

    fn row<V: std::fmt::Display>(&mut self, value: V) {
        let indent = if self.indent { "  " } else { "" };
        println!("{}{}", indent, value);
    }
}

#[derive(clap::ValueEnum, Clone, Copy)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
enum DynamicEntry {
    Rpath,
    Runpath,
}

impl From<DynamicEntry> for elfie::DynamicTag {
    fn from(other: DynamicEntry) -> Self {
        match other {
            DynamicEntry::Rpath => Self::Rpath,
            DynamicEntry::Runpath => Self::Runpath,
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Default)]
#[clap(rename_all = "snake_case")]
enum What {
    #[default]
    All,
    Header,
    Sections,
    Segments,
}
