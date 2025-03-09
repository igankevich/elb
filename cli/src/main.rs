use clap::Parser;
use std::ffi::CStr;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::process::ExitCode;

use elfie::Elf;
use elfie::SectionFlags;
use elfie::SectionKind;
use elfie::SegmentFlags;
use elfie::SegmentKind;
use fs_err::File;
use fs_err::OpenOptions;

#[derive(clap::Parser)]
#[clap(version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Show file contents.
    Show {
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
    Patch {
        /// Set interpreter.
        #[clap(short = 'i', long = "set-interpreter", value_name = "file")]
        set_interpreter: Option<PathBuf>,

        /// Remove interpreter.
        #[clap(action, long = "remove-interpreter", value_name = "file")]
        remove_interpreter: bool,

        /// ELF file.
        #[clap(value_name = "ELF file")]
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    do_main()
        .inspect_err(|e| eprintln!("{e}"))
        .unwrap_or(ExitCode::FAILURE)
}

fn do_main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let args = Args::parse();
    env_logger::init();
    match args.command {
        Command::Show { file } => show(file),
        Command::Check { file } => check(file),
        Command::Patch {
            set_interpreter,
            remove_interpreter,
            file,
        } => patch(file, set_interpreter, remove_interpreter),
    }
}

fn show(file: PathBuf) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut file = File::open(&file)?;
    let elf = Elf::read_unchecked(&mut file)?;
    println!("Elf:");
    println!("  Class: {:?}", elf.header.class);
    println!("  Byte order: {:?}", elf.header.byte_order);
    println!("  OS ABI: {:?}", elf.header.os_abi);
    println!("  ABI version: {:?}", elf.header.abi_version);
    println!("  File type: {:?}", elf.header.kind);
    println!("  Machine: {:?}", elf.header.machine);
    println!("  Flags: {:#x}", elf.header.flags);
    println!("  Entry point: {:#x}", elf.header.entry_point);
    println!(
        "  Program header: {:#x}..{:#x}",
        elf.header.program_header_offset,
        elf.header.program_header_offset
            + elf.header.num_segments as u64 * elf.header.segment_len as u64,
    );
    println!(
        "  Section header: {:#x}..{:#x}",
        elf.header.section_header_offset,
        elf.header.section_header_offset
            + elf.header.num_sections as u64 * elf.header.section_len as u64,
    );
    println!("\nSections:");
    if !elf.sections.is_empty() {
        println!(
            "  {:20}  {:38}  {:38}  Flags      Type",
            "Name", "File block", "Memory block"
        );
    }
    let names_section = elf.sections.get(elf.header.section_names_index as usize);
    let names = if let Some(names_section) = names_section {
        names_section.read_content(&mut file)?
    } else {
        Vec::new()
    };
    for section in elf.sections.iter() {
        let memory_start = section.virtual_address;
        let memory_end = memory_start + section.size;
        let file_start = section.offset;
        let file_end = file_start + section.size;
        let name_bytes = names.get(section.name_offset as usize..).unwrap_or(&[]);
        let name_end = name_bytes.iter().position(|ch| *ch == 0);
        let name = String::from_utf8_lossy(&name_bytes[..name_end.unwrap_or(0)]);
        println!(
            "  {:20}  {:#018x}..{:#018x}  {:#018x}..{:#018x}  {}  {}",
            name,
            file_start,
            file_end,
            memory_start,
            memory_end,
            SectionFlagsStr(section.flags),
            SectionKindStr(section.kind)
        );
    }
    println!("\nSection flags:");
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
    println!("\nSegments:");
    if !elf.sections.is_empty() {
        println!(
            "  {:20}  {:38}  {:38}  Flags",
            "Type", "File block", "Memory block"
        );
    }
    for segment in elf.segments.iter() {
        let memory_start = segment.virtual_address;
        let memory_end = memory_start + segment.memory_size;
        let file_start = segment.offset;
        let file_end = file_start + segment.file_size;
        println!(
            "  {:20}  {:#018x}..{:#018x}  {:#018x}..{:#018x}  {}",
            SegmentKindStr(segment.kind),
            file_start,
            file_end,
            memory_start,
            memory_end,
            SegmentFlagsStr(segment.flags),
        );
    }
    elf.validate()?;
    // TODO segment-to-section mapping
    Ok(ExitCode::SUCCESS)
}

fn check(file: PathBuf) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut file = File::open(&file)?;
    let _elf = Elf::read(&mut file)?;
    Ok(ExitCode::SUCCESS)
}

fn patch(
    path: PathBuf,
    set_interpreter: Option<PathBuf>,
    remove_interpreter: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut elf = Elf::read(File::open(&path)?)?;
    let mut changed = false;
    let file_name = path.file_name().expect("File name exists");
    let new_file_name = {
        let mut name = OsString::new();
        name.push(".");
        name.push(file_name);
        name.push(".tmp");
        name
    };
    let new_path = match path.parent() {
        Some(parent) => parent.join(&new_file_name),
        None => new_file_name.into(),
    };
    let _ = std::fs::remove_file(&new_path);
    fs_err::copy(&path, &new_path)?;
    let mut file = OpenOptions::new().read(true).write(true).open(&new_path)?;
    if remove_interpreter {
        elf.remove_interpreter(&mut file)?;
        changed = true;
    } else if let Some(path) = set_interpreter {
        let os_string = path.into_os_string();
        let mut bytes = os_string.into_vec();
        bytes.push(0_u8);
        let c_str = CStr::from_bytes_with_nul(&bytes)?;
        elf.set_interpreter(&mut file, c_str)?;
        changed = true;
    }
    if !changed {
        return Err("No option selected".into());
    }
    elf.write(&mut file)?;
    fs_err::rename(&new_path, &path)?;
    Ok(ExitCode::SUCCESS)
}

struct SectionKindStr(SectionKind);

impl std::fmt::Display for SectionKindStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use SectionKind::*;
        let s = match self.0 {
            Null => Some("NULL"),
            ProgramData => Some("PROGBITS"),
            SymbolTable => Some("SYMTAB"),
            StringTable => Some("STRTAB"),
            RelaTable => Some("RELA"),
            Hash => Some("HASH"),
            Dynamic => Some("DYNAMIC"),
            Note => Some("NOTE"),
            NoBits => Some("NOBITS"),
            RelTable => Some("REL"),
            Shlib => Some("SHLIB"),
            DynamicSymbolTable => Some("DYNSYM"),
            InitArray => Some("INIT_ARRAY"),
            FiniArray => Some("FINI_ARRAY"),
            PreInitArray => Some("PREINIT_ARRAY"),
            Group => Some("GROUP"),
            SymbolTableIndex => Some("SYMTAB_SHNDX"),
            RelrTable => Some("RELR"),
            OsSpecific(0x6ffffff5) => Some("GNU_ATTRIBUTES"),
            OsSpecific(0x6ffffff6) => Some("GNU_HASH"),
            OsSpecific(0x6ffffff7) => Some("GNU_LIBLIST"),
            OsSpecific(0x6ffffff8) => Some("CHECKSUM"),
            OsSpecific(0x6ffffffd) => Some("GNU_VERDEF"),
            OsSpecific(0x6ffffffe) => Some("GNU_VERNEED"),
            OsSpecific(0x6fffffff) => Some("GNU_VERSYM"),
            _ => None,
        };
        match s {
            Some(s) => write!(f, "{}", s),
            None => write!(f, "{:#x}", self.0.as_u32()),
        }
    }
}

struct SegmentKindStr(SegmentKind);

impl std::fmt::Display for SegmentKindStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use SegmentKind::*;
        let s = match self.0 {
            Null => Some("NULL"),
            Loadable => Some("LOAD"),
            Dynamic => Some("DYNAMIC"),
            Interpreter => Some("INTERP"),
            Note => Some("NOTE"),
            Shlib => Some("SHLIB"),
            ProgramHeader => Some("PHDR"),
            Tls => Some("TLS"),
            OsSpecific(0x6474e550) => Some("GNU_EH_FRAME"),
            OsSpecific(0x6474e551) => Some("GNU_STACK"),
            OsSpecific(0x6474e552) => Some("GNU_RELRO"),
            OsSpecific(0x6474e553) => Some("GNU_PROPERTY"),
            OsSpecific(0x6474e554) => Some("GNU_SFRAME"),
            _ => None,
        };
        let width = f.width().unwrap_or(0);
        match s {
            Some(s) => write!(f, "{:width$}", s, width = width),
            None => write!(f, "{:#width$x}", self.0.as_u32(), width = width),
        }
    }
}

struct SegmentFlagsStr(SegmentFlags);

impl std::fmt::Display for SegmentFlagsStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut flags_str = [b'-', b'-', b'-', b' '];
        for flag in self.0.iter() {
            match flag {
                SegmentFlags::READABLE => flags_str[0] = b'r',
                SegmentFlags::WRITABLE => flags_str[1] = b'w',
                SegmentFlags::EXECUTABLE => flags_str[2] = b'x',
                _ => flags_str[3] = b'*',
            }
        }
        let s = std::str::from_utf8(&flags_str[..]).expect("The string is UTF-8");
        write!(f, "{}", s)
    }
}

struct SectionFlagsStr(SectionFlags);

impl std::fmt::Display for SectionFlagsStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut flags_str = [b'-', b'-', b'-', b'-', b'-', b'-', b'-', b'-', b' '];
        for flag in self.0.iter() {
            match flag {
                SectionFlags::WRITE => flags_str[0] = b'w',
                SectionFlags::ALLOC => flags_str[1] = b'a',
                SectionFlags::EXECINSTR => flags_str[2] = b'x',
                SectionFlags::MERGE => flags_str[3] = b'm',
                SectionFlags::STRINGS => flags_str[4] = b's',
                SectionFlags::INFO_LINK => flags_str[5] = b'i',
                SectionFlags::LINK_ORDER => flags_str[5] = b'l',
                SectionFlags::OS_NONCONFORMING => flags_str[6] = b'o',
                SectionFlags::GROUP => flags_str[6] = b'g',
                SectionFlags::TLS => flags_str[6] = b't',
                SectionFlags::COMPRESSED => flags_str[7] = b'c',
                _ => flags_str[8] = b'*',
            }
        }
        let s = std::str::from_utf8(&flags_str[..]).expect("The string is UTF-8");
        write!(f, "{}", s)
    }
}
