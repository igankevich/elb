use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

use elb::Elf;
use fs_err::File;

mod deps;
mod formatting;
mod logger;
mod patch;
mod show;

use self::deps::*;
use self::formatting::*;
use self::logger::*;
use self::patch::*;
use self::show::*;

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
    Show(ShowArgs),
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
pub struct CommonArgs {
    /// Memory page size.
    #[clap(long = "page-size", value_name = "NUM", default_value_t = 4096)]
    page_size: u64,
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
        Command::Show(show_args) => show(args.common, show_args),
        Command::Check { file } => check(args.common, file),
        Command::Deps(deps_args) => deps(args.common, deps_args),
        Command::Patch(patch_args) => patch(args.common, patch_args),
    }
}

fn check(common: CommonArgs, file: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&file)?;
    let _elf = Elf::read(&mut file, common.page_size)?;
    Ok(())
}
