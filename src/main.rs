#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Optional name to operate on
    name: Option<String>,

    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize new repository in current directory
    Init,
    CatFile {
        #[arg(short = 'p')]
        blob_sha: String,
    },
}

fn g_init() -> Result<()> {
    fs::create_dir(".git")?;
    fs::create_dir(".git/objects")?;
    fs::create_dir(".git/refs")?;
    fs::write(".git/HEAD", "ref: refs/heads/master\n")?;
    println!("Initialized git directory");
    Ok(())
}

fn p_cat_file(blob_sha: &str) -> Result<()> {
    let path = ".git/objects/".to_string()
        + blob_sha.chars().take(2).collect::<String>().as_str()
        + "/"
        + blob_sha.chars().skip(2).collect::<String>().as_str();
    let content = fs::read(path)?;
    let mut decompressor = flate2::read::ZlibDecoder::new(&content[..]);
    let mut result = String::new();
    decompressor.read_to_string(&mut result)?;
    let (header, content) = result
        .split_once("\0")
        .ok_or(anyhow::anyhow!("Couldn't parse git object {blob_sha}"))?;
    print!("{content}");
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    // println!("Logs from your program will appear here!");

    match &cli.command {
        Commands::Init => g_init(),
        Commands::CatFile { blob_sha } => p_cat_file(blob_sha),
    }
}
