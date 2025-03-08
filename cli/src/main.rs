use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

use elfie::Elf;
use elfie::SectionFlags;
use elfie::SectionKind;
use elfie::SegmentFlags;
use elfie::SegmentKind;
use fs_err::File;

#[derive(clap::Parser)]
#[clap(version)]
struct Args {
    /// ELF file.
    #[clap(value_name = "ELF file")]
    file: PathBuf,
}

fn main() -> ExitCode {
    do_main()
        .inspect_err(|e| eprintln!("{e}"))
        .unwrap_or(ExitCode::FAILURE)
}

fn do_main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut file = File::open(&args.file)?;
    let elf = Elf::read(&mut file)?;
    println!("Elf:");
    println!("  Class: {:?}", elf.header.class);
    println!("  Byte order: {:?}", elf.header.byte_order);
    println!("  OS ABI: {:?}", elf.header.os_abi);
    println!("  ABI version: {:?}", elf.header.abi_version);
    println!("  File type: {:?}", elf.header.kind);
    println!("  Machine: {:?}", elf.header.machine);
    println!("  Flags: {:#x}", elf.header.flags);
    println!("  Entry point: {:#x}", elf.header.entry_point.as_u64());
    println!(
        "  Program header: {:#x}..{:#x}",
        elf.header.program_header_offset.as_u64(),
        elf.header.program_header_offset.as_u64()
            + elf.header.num_segments as u64 * elf.header.segment_len as u64,
    );
    println!(
        "  Section header: {:#x}..{:#x}",
        elf.header.section_header_offset.as_u64(),
        elf.header.section_header_offset.as_u64()
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
        let memory_start = section.virtual_address.as_u64();
        let memory_end = memory_start + section.size.as_u64();
        let file_start = section.offset.as_u64();
        let file_end = file_start + section.size.as_u64();
        let name_bytes = names.get(section.name as usize..).unwrap_or(&[]);
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
        let memory_start = segment.virtual_address.as_u64();
        let memory_end = memory_start + segment.memory_size.as_u64();
        let file_start = segment.offset.as_u64();
        let file_end = file_start + segment.file_size.as_u64();
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
            Interpretator => Some("INTERP"),
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
