use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use self::commands::cat_file;
use self::object::{hash_object, write_tree};
mod commands;
mod hash;
mod object;
mod storage;

#[derive(Parser)]
#[command(name = "rgit", about = "A mini-git implementation in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Initialize a new rgit repository
    Init,
    HashObject {
        file: PathBuf,
        #[arg(short = 'w')]
        write: bool,
    },
    CatFile {
        #[arg(short = 'p')]
        pretty_print: bool,
        object_hash: String,
    },
    WriteTree,
}
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => {
            commands::init()?;
        }
        Commands::HashObject { file, write } => {
            let sha1 = hash_object(&file, write)?;
            println!("{}", sha1);
        }
        Commands::CatFile {
            pretty_print,
            object_hash,
        } => {
            cat_file(&object_hash, pretty_print)?;
        }
        Commands::WriteTree => {
            write_tree(Path::new(".rgit"))?;
        }
    }
    Ok(())
}
