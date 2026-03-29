use std::path::PathBuf;

use clap::{Parser, Subcommand};

use self::commands::hash_object;
mod commands;

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
    }
    Ok(())
}
