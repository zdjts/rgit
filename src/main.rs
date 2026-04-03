use std::path::PathBuf;

use clap::{Parser, Subcommand};

use self::commands::cat_file;
use self::index::Index;
use self::object::{commit_tree, hash_object, write_tree_from_index};
use self::refs::{head_ref, resolve_ref, set_head, update_ref};
mod commands;
mod hash;
mod index;
mod object;
mod refs;
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
    UpdateIndex {
        path: PathBuf,
        /// Mode (e.g., 100644 for regular file, 100755 for executable)
        #[arg(short = 'm', long, default_value = "100644")]
        mode: u32,
    },
    CommitTree {
        tree: String,
        /// Parent commit hash(es)
        #[arg(short = 'p', long)]
        parent: Vec<String>,
        /// Author information "Name <email>"
        #[arg(short, long)]
        author: String,
        /// Commit message
        #[arg(short, long)]
        message: String,
    },
    /// Update a ref to point to a commit hash
    UpdateRef {
        /// Ref name, e.g. refs/heads/master
        refname: String,
        /// Commit hash to point to
        hash: String,
    },
    /// Set HEAD to a symbolic ref or detached commit hash
    SetHead {
        /// A ref name (refs/heads/master) or a commit hash for detached HEAD
        target: String,
    },
    /// Show what HEAD currently points to (resolved to commit hash)
    ShowRef,
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
            let index = Index::load()?;
            let sha1 = write_tree_from_index(&index)?;
            println!("{}", sha1);
        }
        Commands::UpdateIndex { path, mode } => {
            let mut index = Index::load()?;
            // Hash the file and get its SHA-1
            let sha1_str = hash_object(&path, true)?;
            let sha1_bytes = hex::decode(&sha1_str)?;
            let mut sha1_arr = [0u8; 20];
            sha1_arr.copy_from_slice(&sha1_bytes);

            // Convert path to relative string
            let path_str = path.to_string_lossy().to_string();
            index.add(&path_str, mode, sha1_arr);
            index.save()?;
            println!("{}", sha1_str);
        }
        Commands::CommitTree {
            tree,
            parent,
            author,
            message,
        } => {
            let sha1 = commit_tree(&tree, &parent, &author, &message)?;
            println!("{}", sha1);
        }
        Commands::UpdateRef { refname, hash } => {
            update_ref(&refname, &hash)?;
        }
        Commands::SetHead { target } => {
            set_head(&target)?;
        }
        Commands::ShowRef => {
            match head_ref()? {
                Some(refname) => {
                    print!("HEAD -> {}", refname);
                    match resolve_ref(&refname)? {
                        Some(hash) => println!(" -> {}", hash),
                        None => println!(" (未初始化)"),
                    }
                }
                None => {
                    // detached HEAD
                    match resolve_ref("HEAD")? {
                        Some(hash) => println!("HEAD (detached) -> {}", hash),
                        None => println!("HEAD 未设置"),
                    }
                }
            }
        }
    }
    Ok(())
}
