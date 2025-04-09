use std::path::PathBuf;

use elb_dl::ElfRelocator;

use crate::CommonArgs;
use crate::LoaderArgs;

#[derive(clap::Args)]
pub struct RelocateArgs {
    #[clap(flatten)]
    loader: LoaderArgs,

    /// Target directory.
    #[clap(short = 't', long = "target", value_name = "DIR")]
    target_dir: PathBuf,

    /// ELF file(s).
    #[clap(value_name = "FILE...")]
    files: Vec<PathBuf>,
}

pub fn relocate(common: CommonArgs, args: RelocateArgs) -> Result<(), Box<dyn std::error::Error>> {
    let loader = args.loader.new_loader(common.page_size)?;
    let relocator = ElfRelocator::new(loader);
    for file in args.files.into_iter() {
        relocator.relocate(file, &args.target_dir)?;
    }
    Ok(())
}
