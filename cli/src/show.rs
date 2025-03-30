use colored::Colorize;
use std::io::BufWriter;
use std::io::Stdout;
use std::io::Write;
use std::path::PathBuf;

use elb::ArmFlags;
use elb::BlockRead;
use elb::Elf;
use elb::ElfSeek;
use elb::Machine;
use elb::SectionKind;
use elb::StringTable;
use elb::SymbolTable;
use fs_err::File;

use crate::CommonArgs;
use crate::SectionFlagsStr;
use crate::SectionKindStr;
use crate::SegmentFlagsStr;
use crate::SegmentKindStr;
use crate::SymbolBindingStr;
use crate::SymbolKindStr;
use crate::SymbolVisibilityStr;

#[derive(clap::Args)]
pub struct ShowArgs {
    /// What to show?
    #[clap(short = 't', default_value = "all")]
    what: What,

    /// ELF file.
    #[clap(value_name = "ELF file")]
    file: PathBuf,
}

pub fn show(common: CommonArgs, args: ShowArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&args.file)?;
    let elf = Elf::read_unchecked(&mut file, common.page_size)?;
    let section_names = elf.read_section_names(&mut file)?.unwrap_or_default();
    match args.what {
        What::Header => {
            let mut printer = Printer::new(false);
            show_header(&elf, &mut printer);
        }
        What::Sections => {
            let mut printer = Printer::new(true);
            printer.title("Sections");
            show_sections(&elf, &section_names, &mut printer)?;
        }
        What::Segments => {
            let mut printer = Printer::new(false);
            show_segments(&elf, &section_names, &mut printer)?;
        }
        What::Symbols => {
            let mut printer = Printer::new(true);
            show_symbols(&elf, &section_names, &mut file, &mut printer)?;
        }
        What::All => {
            let mut printer = Printer::new(true);
            printer.title("Header");
            show_header(&elf, &mut printer);
            printer.title("Sections");
            show_sections(&elf, &section_names, &mut printer)?;
            printer.title("Segments");
            show_segments(&elf, &section_names, &mut printer)?;
            show_symbols(&elf, &section_names, &mut file, &mut printer)?;
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
        let file_offsets = section.file_offset_range();
        let name_bytes = names
            .get_string(section.name_offset as usize)
            .unwrap_or_default();
        let name = String::from_utf8_lossy(name_bytes.to_bytes());
        printer.row(format_args!(
            "{:20}  {:#018x}..{:#018x}  {:#018x}..{:#018x}  {}  {}",
            name,
            file_offsets.start,
            file_offsets.end,
            memory_start,
            memory_end,
            SectionFlagsStr(section.flags),
            SectionKindStr(section.kind)
        ));
    }
    printer.title("Section flags");
    printer.line("  w  Writable");
    printer.line("  a  Occupies memory during execution");
    printer.line("  x  Executable");
    printer.line("  m  Mergeable");
    printer.line("  s  Contains NUL-terminated strings");
    printer.line("  i  Linked to another section");
    printer.line("  l  Preserve order after combining");
    printer.line("  o  OS specific handling required");
    printer.line("  g  Group member");
    printer.line("  t  Holds thread-local data");
    printer.line("  c  Compressed");
    printer.line("  *  Unknown flags");
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
            if (file_start..file_end).contains(&section.offset)
                || (memory_start..memory_end).contains(&section.virtual_address)
            {
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

fn show_symbols(
    elf: &Elf,
    names: &StringTable,
    file: &mut File,
    printer: &mut Printer,
) -> Result<(), Box<dyn std::error::Error>> {
    for section in elf.sections.iter() {
        if !matches!(
            section.kind,
            SectionKind::SymbolTable | SectionKind::DynamicSymbolTable
        ) {
            continue;
        }
        let name = names
            .get_string(section.name_offset as usize)
            .unwrap_or_default();
        file.seek(section.offset)?;
        let symbol_table =
            SymbolTable::read(file, elf.header.class, elf.header.byte_order, section.size)?;
        if symbol_table.is_empty() {
            continue;
        }
        let strings: StringTable = {
            let Some(section) = elf.sections.get(section.link as usize) else {
                continue;
            };
            section.read_content(file, elf.header.class, elf.header.byte_order)?
        };
        printer.title(&format!("Symbols from {:?}", name));
        if !elf.sections.is_empty() {
            printer.row(format_args!(
                "{:20}  {:>10}  {:7}  {:8}  {:9}  {:20}  Name",
                "Address", "Size", "Binding", "Type", "Visibility", "Section"
            ));
        }
        for symbol in symbol_table.iter() {
            let name = strings
                .get_string(symbol.name_offset as usize)
                .unwrap_or_default();
            let name = std::str::from_utf8(name.to_bytes()).unwrap_or_default();
            let section_name = elf
                .sections
                .get(symbol.section_index as usize)
                .and_then(|section| names.get_string(section.name_offset as usize))
                .unwrap_or_default();
            let section_name = std::str::from_utf8(section_name.to_bytes()).unwrap_or_default();
            printer.row(format_args!(
                "{:#020x}  {:10}  {:7}  {:8}  {:9}  {:20}  {}",
                symbol.address,
                symbol.size,
                SymbolBindingStr(symbol.binding),
                SymbolKindStr(symbol.kind),
                SymbolVisibilityStr(symbol.visibility),
                section_name,
                name,
            ));
        }
    }
    Ok(())
}

struct Printer {
    first_title: bool,
    indent: bool,
    writer: BufWriter<Stdout>,
}

impl Printer {
    fn new(indent: bool) -> Self {
        Self {
            first_title: true,
            indent,
            writer: BufWriter::new(std::io::stdout()),
        }
    }

    fn title(&mut self, title: &str) {
        let newline = if !self.first_title {
            "\n"
        } else {
            self.first_title = false;
            ""
        };
        let _ = writeln!(self.writer, "{}{}", newline, title.bold().underline());
    }

    fn kv<V: std::fmt::Display>(&mut self, key: &str, value: V) {
        let indent = if self.indent { "  " } else { "" };
        let _ = writeln!(self.writer, "{}{}: {}", indent, key.bold().blue(), value);
    }

    fn row<V: std::fmt::Display>(&mut self, value: V) {
        let indent = if self.indent { "  " } else { "" };
        let _ = writeln!(self.writer, "{}{}", indent, value);
    }

    fn line<V: std::fmt::Display>(&mut self, value: V) {
        let _ = writeln!(self.writer, "{}", value);
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
    Symbols,
}
