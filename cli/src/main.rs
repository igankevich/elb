use clap::Parser;
use clap::ValueEnum;
use colored::Colorize;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsString;
use std::io::Read;
use std::io::Seek;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::process::ExitCode;

use elfie::Elf;
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
    /// Modify ELF file.
    Patch(PatchArgs),
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
    add_dynamic: Vec<String>,

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
        Command::Show { what, file } => show(what, file),
        Command::Check { file } => check(file),
        Command::Patch(patch_args) => patch(patch_args),
    }
}

fn show(what: What, file: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&file)?;
    let elf = Elf::read_unchecked(&mut file)?;
    match what {
        What::Header => {
            let mut printer = Printer::new(false);
            show_header(&elf, &mut printer);
        }
        What::Sections => {
            let mut printer = Printer::new(false);
            show_sections(&elf, &mut file, &mut printer)?;
        }
        What::Segments => {
            let mut printer = Printer::new(false);
            show_segments(&elf, &mut printer)?;
        }
        What::All => {
            let mut printer = Printer::new(true);
            printer.title("Header");
            show_header(&elf, &mut printer);
            printer.title("Sections");
            show_sections(&elf, &mut file, &mut printer)?;
            printer.title("Segments");
            show_segments(&elf, &mut printer)?;
        }
    }
    Ok(())
}

fn show_header(elf: &Elf, printer: &mut Printer) {
    printer.kv("Class", format_args!("{:?}", elf.header.class));
    printer.kv("Byte order", format_args!("{:?}", elf.header.byte_order));
    printer.kv("OS ABI", format_args!("{:?}", elf.header.os_abi));
    printer.kv("ABI version", format_args!("{:?}", elf.header.abi_version));
    printer.kv("File type", format_args!("{:?}", elf.header.kind));
    printer.kv("Machine", format_args!("{:?}", elf.header.machine));
    printer.kv("Flags", format_args!("{:#x}", elf.header.flags));
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

fn show_sections<R: Read + Seek>(
    elf: &Elf,
    mut file: R,
    printer: &mut Printer,
) -> Result<(), Box<dyn std::error::Error>> {
    if !elf.sections.is_empty() {
        printer.row(format_args!(
            "{:20}  {:38}  {:38}  Flags      Type",
            "Name", "File block", "Memory block"
        ));
    }
    let names = elf.read_section_names(&mut file)?;
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

fn show_segments(elf: &Elf, printer: &mut Printer) -> Result<(), Box<dyn std::error::Error>> {
    if !elf.sections.is_empty() {
        printer.row(format_args!(
            "{:20}  {:38}  {:38}  Flags",
            "Type", "File block", "Memory block"
        ));
    }
    for segment in elf.segments.iter() {
        let memory_start = segment.virtual_address;
        let memory_end = memory_start + segment.memory_size;
        let file_start = segment.offset;
        let file_end = file_start + segment.file_size;
        printer.row(format_args!(
            "{:20}  {:#018x}..{:#018x}  {:#018x}..{:#018x}  {}",
            SegmentKindStr(segment.kind),
            file_start,
            file_end,
            memory_start,
            memory_end,
            SegmentFlagsStr(segment.flags),
        ));
    }
    elf.validate()?;
    // TODO segment-to-section mapping
    Ok(())
}

fn check(file: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&file)?;
    let _elf = Elf::read(&mut file)?;
    Ok(())
}

fn patch(args: PatchArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut elf = Elf::read(File::open(&args.file)?)?;
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
    let mut file = OpenOptions::new().read(true).write(true).open(&new_path)?;
    if args.remove_interpreter {
        elf.remove_interpreter(&mut file)?;
        changed = true;
    } else if let Some(path) = args.set_interpreter {
        let os_string = path.into_os_string();
        let mut bytes = os_string.into_vec();
        bytes.push(0_u8);
        let c_str = CStr::from_bytes_with_nul(&bytes)?;
        elf.set_interpreter(&mut file, c_str)?;
        changed = true;
    }
    for entry in args.remove_dynamic.into_iter() {
        elf.remove_dynamic(&mut file, entry.into())?;
        changed = true;
    }
    for pair in args.add_dynamic.into_iter() {
        let mut iter = pair.splitn(2, '=');
        let tag: DynamicEntry = ValueEnum::from_str(iter.next().ok_or("Tag not found")?, true)?;
        let mut value = iter.next().ok_or("Value not found")?.as_bytes().to_vec();
        value.push(0_u8);
        let value = CString::from_vec_with_nul(value)?;
        elf.add_dynamic_c_str(&mut file, tag.into(), &value)?;
        changed = true;
    }
    if !changed {
        return Err("No changes".into());
    }
    elf.write(&mut file)?;
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

impl From<DynamicEntry> for elfie::DynamicEntryKind {
    fn from(other: DynamicEntry) -> Self {
        match other {
            DynamicEntry::Rpath => Self::RpathOffset,
            DynamicEntry::Runpath => Self::RunPathOffset,
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
