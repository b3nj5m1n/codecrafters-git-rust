#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use sha1::{Digest, Sha1};

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
    HashObject {
        #[arg(short = 'w')]
        file: PathBuf,
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

fn sha_to_path(sha: &str) -> Result<PathBuf> {
    let mut result = PathBuf::from(".git/objects/");
    result.push(sha.chars().take(2).collect::<String>());
    result.push(sha.chars().skip(2).collect::<String>());
    Ok(result)
}

fn p_cat_file(blob_sha: &str) -> Result<()> {
    let path = sha_to_path(blob_sha)?;
    let content = fs::read(path)?;
    let object = Object::try_from(content)?;
    std::io::stdout().write_all(&object.content)?;
    // print!("{}", String::from_utf8(object.content));
    Ok(())
}

#[derive(Clone)]
struct Object {
    object_type: ObjectType,
    size: usize,
    content: Vec<u8>,
}

impl Object {
    fn new(object_type: ObjectType, content: String) -> Self {
        Self {
            object_type,
            size: content.len(),
            content: content.as_bytes().to_vec(),
        }
    }
    fn new_blob(content: String) -> Self {
        Self::new(ObjectType::Blob, content)
    }
    fn hash(&self) -> String {
        let mut hasher = Sha1::new();
        hasher.update(Into::<Vec<u8>>::into(self.clone()));
        hex::encode(hasher.finalize())
    }
    fn compress(&self) -> Result<Vec<u8>> {
        let mut compressor = flate2::read::ZlibEncoder::new(
            std::io::Cursor::new(Into::<Vec<u8>>::into(self.clone())),
            flate2::Compression::fast(),
        );
        let mut result = Vec::new();
        compressor.read_to_end(&mut result)?;
        Ok(result)
    }
}

impl Into<Vec<u8>> for Object {
    fn into(self) -> Vec<u8> {
        let mut r = format!("{0} {1}\0", self.object_type.to_string(), self.size,).into_bytes();
        r.append(&mut self.content.clone());
        r
        // self.content
    }
}

impl TryFrom<Vec<u8>> for Object {
    type Error = anyhow::Error;

    fn try_from(value: Vec<u8>) -> std::result::Result<Self, Self::Error> {
        let mut decompressor = flate2::read::ZlibDecoder::new(&value[..]);
        let mut result: Vec<u8> = Vec::new(); // String::new();
        decompressor.read_to_end(&mut result)?;
        // decompressor.read_to_string(&mut result)?;
        let (header, content) = result.split_at(
            result
                .iter()
                .position(|&x| x == 0_u8)
                .ok_or(anyhow::anyhow!("Couldn't parse header of object"))?,
        );
        let header = String::from_utf8(header.to_vec())?;

        // .split_once(\0)
        // .ok_or(anyhow::anyhow!("Couldn't parse git object"))?;
        let (object_type_str, size_str) = header
            .split_once(" ")
            .ok_or(anyhow::anyhow!("Couldn't parse git object header"))?;
        let size = size_str.parse::<usize>()?;
        let object_type = ObjectType::try_from(object_type_str)?;
        Ok(Self {
            object_type,
            size,
            content: content.to_vec(),
        })
    }
}

#[derive(Clone)]
enum ObjectType {
    Blob,
    Tree,
    Commit,
}

impl ToString for ObjectType {
    fn to_string(&self) -> String {
        match self {
            ObjectType::Blob => String::from("blob"),
            ObjectType::Tree => String::from("tree"),
            ObjectType::Commit => String::from("commit"),
        }
    }
}

impl TryFrom<&str> for ObjectType {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value {
            "blob" => Ok(Self::Blob),
            "tree" => Ok(Self::Tree),
            "commit" => Ok(Self::Commit),
            _ => anyhow::bail!("Couldn't determine object type: {}", value.to_string()),
        }
    }
}

fn p_hash_object(file: &PathBuf) -> Result<()> {
    anyhow::ensure!(file.exists());
    let content = fs::read(file)?;
    let object = Object::new_blob(std::string::String::from_utf8(content)?);
    let hash = object.hash();
    let compressed = object.compress()?;
    let path = sha_to_path(&hash)?;
    std::fs::create_dir_all(&path.parent().ok_or(anyhow::anyhow!("Unreachable"))?)?;
    let mut file = File::create(path)?;
    file.write_all(&compressed)?;
    println!("{hash}");
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => g_init(),
        Commands::CatFile { blob_sha } => p_cat_file(blob_sha),
        Commands::HashObject { file } => p_hash_object(file),
    }
}
